//! The convenience driver: `HostHandler` and `AsyncHostHandler` over the
//! event loop.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    AsyncHostHandler, BoxFuture, Call, Engine, EngineConfig, HostError, HostErrorKind, HostHandler,
    HostModule, HostValue, ParkReason, Progress, Resume, Session, Step,
};
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

const SRC: &str = "\
import asyncio
total = 0
for i in range(300):
    total += add(i, 1)
async def main():
    return await asyncio.gather(slow(1), slow(2), slow(3))
try:
    fail()
except ValueError as e:
    print('caught', e)
(total, asyncio.run(main()))
";

/// Answers by function id: `add` sums, `slow` returns a future settled at
/// the next wait (twice its argument), `fail` raises. Refuels fuel parks.
struct Tools {
    add: u32,
    slow: u32,
    fail: u32,
    pending: Vec<(u64, i64)>,
    log: Vec<String>,
}

impl Tools {
    fn new(s: &mut Session) -> Self {
        let ids = s
            .define(&HostModule::untyped("", &["add", "slow", "fail"]))
            .expect("defined");
        let id = |n: &str| ids.iter().find(|f| f.name == n).expect("id").id;
        Tools {
            add: id("add"),
            slow: id("slow"),
            fail: id("fail"),
            pending: Vec::new(),
            log: Vec::new(),
        }
    }

    fn answer(&mut self, call: Call<'_>) -> Step {
        let int = |v: &HostValue| match v {
            HostValue::Int(i) => *i,
            other => panic!("{other:?}"),
        };
        Step::Resume(match call.function {
            f if f == self.add => {
                Resume::Value(HostValue::Int(int(&call.args[0]) + int(&call.args[1])))
            }
            f if f == self.slow => {
                self.pending.push((call.call, 2 * int(&call.args[0])));
                Resume::Future
            }
            f if f == self.fail => Resume::Error(HostError {
                kind: HostErrorKind::ValueError,
                message: "no".to_owned(),
                data: None,
                class: None,
            }),
            _ => panic!("unknown function {}", call.name),
        })
    }

    fn batch(&mut self, calls: &[u64]) -> Step {
        self.log.push(format!("waiting {}", calls.len()));
        Step::Resume(Resume::Batch(
            self.pending
                .drain(..)
                .map(|(c, v)| (c, Ok(HostValue::Int(v))))
                .collect(),
        ))
    }

    fn park(&mut self, session: &mut Session, reason: ParkReason) -> Step {
        self.log.push(format!("{reason:?}"));
        session.set_fuel(Some(500));
        Step::Resume(Resume::Continue)
    }
}

impl HostHandler for Tools {
    fn host_call(&mut self, _: &mut Session, call: Call<'_>) -> Step {
        self.answer(call)
    }
    fn waiting(&mut self, _: &mut Session, calls: &[u64]) -> Step {
        self.batch(calls)
    }
    fn parked(&mut self, session: &mut Session, reason: ParkReason) -> Step {
        self.park(session, reason)
    }
}

impl AsyncHostHandler for Tools {
    fn host_call<'a>(&'a mut self, _: &'a mut Session, call: Call<'a>) -> BoxFuture<'a, Step> {
        Box::pin(async move { self.answer(call) })
    }
    fn waiting<'a>(&'a mut self, _: &'a mut Session, calls: &'a [u64]) -> BoxFuture<'a, Step> {
        Box::pin(async move { self.batch(calls) })
    }
    fn parked<'a>(&'a mut self, s: &'a mut Session, reason: ParkReason) -> BoxFuture<'a, Step> {
        Box::pin(async move { self.park(s, reason) })
    }
}

fn setup() -> (Engine, Session, Tools) {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    s.set_fuel(Some(100));
    let tools = Tools::new(&mut s);
    (engine, s, tools)
}

fn check(p: &Progress, tools: &Tools) {
    assert!(
        matches!(p, Progress::Complete(Some(c)) if c.repr == "(45150, [2, 4, 6])"),
        "{p:?}"
    );
    assert!(
        tools.log.iter().filter(|l| *l == "Fuel").count() >= 1,
        "{:?}",
        tools.log
    );
    assert_eq!(tools.log.last().map(String::as_str), Some("waiting 3"));
}

#[test]
fn a_handler_drives_host_calls_futures_and_fuel_parks_to_completion() {
    let (engine, mut s, mut tools) = setup();
    let code = engine.compile(SRC, "cell.py").expect("compiles");
    let p = s.run(&code, &mut tools).expect("drives");
    check(&p, &tools);
}

/// Polls `f` to completion (the handlers never wait on anything).
fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

#[test]
fn an_async_handler_drives_the_same_cell() {
    let (engine, mut s, mut tools) = setup();
    let code = engine.compile(SRC, "cell.py").expect("compiles");
    let start = s.start(&code);
    let p = block_on(s.drive_async(start, &mut tools)).expect("drives");
    check(&p, &tools);
}

/// Answers nothing: stops at the first host call.
struct Stopper;

impl HostHandler for Stopper {
    fn host_call(&mut self, _: &mut Session, _: Call<'_>) -> Step {
        Step::Stop
    }
}

#[test]
fn a_stop_returns_the_progress_and_the_loop_can_take_over() {
    let (engine, mut s, mut tools) = setup();
    let code = engine
        .compile("add(1, 2) + add(3, 4)", "cell.py")
        .expect("compiles");
    let p = s.run(&code, &mut Stopper).expect("drives");
    let Progress::HostCall { name, function, .. } = &p else {
        panic!("{p:?}")
    };
    assert_eq!((name.as_str(), *function), ("add", tools.add));
    // A wrong answer is refused and changes nothing; the session is still
    // at the call, and a handler (or the loop) can continue it.
    assert!(s.drive(p.clone(), &mut WrongAnswer).is_err());
    let p = s.resume(Resume::Value(HostValue::Int(3))).expect("resumes");
    let p = s.drive(p, &mut tools).expect("drives");
    assert!(
        matches!(&p, Progress::Complete(Some(c)) if c.repr == "10"),
        "{p:?}"
    );
}

/// Answers a host call with `Continue`, which only fits a park.
struct WrongAnswer;

impl HostHandler for WrongAnswer {
    fn host_call(&mut self, _: &mut Session, _: Call<'_>) -> Step {
        Step::Resume(Resume::Continue)
    }
}

/// A single-threaded executor: polls each future only after its waker
/// fired, and logs which one returned `Pending`. Panics if every future
/// is pending and none was woken (a lost wakeup).
fn run_all<F: Future + Unpin>(mut futures: Vec<F>) -> (Vec<F::Output>, Vec<usize>) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::Wake;
    struct Flag(AtomicBool);
    impl Wake for Flag {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let flags: Vec<Arc<Flag>> = futures
        .iter()
        .map(|_| Arc::new(Flag(AtomicBool::new(true))))
        .collect();
    let mut out: Vec<Option<F::Output>> = futures.iter().map(|_| None).collect();
    let mut pending_log = Vec::new();
    while out.iter().any(Option::is_none) {
        let mut polled = false;
        for (i, f) in futures.iter_mut().enumerate() {
            if out[i].is_some() || !flags[i].0.swap(false, Ordering::SeqCst) {
                continue;
            }
            polled = true;
            let waker = Waker::from(Arc::clone(&flags[i]));
            match std::pin::Pin::new(f).poll(&mut Context::from_waker(&waker)) {
                Poll::Ready(v) => out[i] = Some(v),
                Poll::Pending => pending_log.push(i),
            }
        }
        assert!(polled, "every future is pending and none was woken");
    }
    (
        out.into_iter().map(|o| o.expect("done")).collect(),
        pending_log,
    )
}

fn assert_send<T: Send>(_: &T) {}

/// Counts real fuel parks and refuels by `refuel`.
struct Refuel {
    refuel: u64,
    parks: usize,
}

impl AsyncHostHandler for Refuel {
    fn host_call<'a>(&'a mut self, _: &'a mut Session, c: Call<'a>) -> BoxFuture<'a, Step> {
        panic!("no host calls: {}", c.name)
    }
    fn parked<'a>(&'a mut self, s: &'a mut Session, reason: ParkReason) -> BoxFuture<'a, Step> {
        assert_eq!(reason, ParkReason::Fuel);
        self.parks += 1;
        s.set_fuel(Some(self.refuel));
        Box::pin(async { Step::Resume(Resume::Continue) })
    }
}

impl HostHandler for Refuel {
    fn host_call(&mut self, _: &mut Session, c: Call<'_>) -> Step {
        panic!("no host calls: {}", c.name)
    }
    fn parked(&mut self, s: &mut Session, reason: ParkReason) -> Step {
        assert_eq!(reason, ParkReason::Fuel);
        self.parks += 1;
        s.set_fuel(Some(self.refuel));
        Step::Resume(Resume::Continue)
    }
}

/// A loop well past one slice, and a native sort whose charge crosses
/// slice boundaries.
const LONG: &str = "\
t = 0
for i in range(60000):
    t += i
d = sorted(range(200000), key=None, reverse=True)
(t, d[0], len(d))
";

#[test]
fn async_sessions_interleave_on_one_thread_in_fuel_slices() {
    let engine = Engine::new(EngineConfig::default());
    let code = engine.compile(LONG, "cell.py").expect("compiles");
    // One session with unlimited fuel, one with a budget it refuels.
    let mut a = Session::new(&engine).expect("a");
    let mut b = Session::new(&engine).expect("b");
    b.set_fuel(Some(25_000));
    let (mut ha, mut hb) = (
        Refuel {
            refuel: 0,
            parks: 0,
        },
        Refuel {
            refuel: 25_000,
            parks: 0,
        },
    );
    let futures = vec![a.run_async(&code, &mut ha), b.run_async(&code, &mut hb)];
    assert_send(&futures[0]);
    let futures: Vec<_> = futures.into_iter().map(Box::pin).collect();
    let (results, pending) = run_all(futures);
    for p in &results {
        let p = p.as_ref().expect("drives");
        assert!(
            matches!(p, Progress::Complete(Some(c)) if c.repr == "(1799970000, 199999, 200000)"),
            "{p:?}"
        );
    }
    // Both yielded many times, and alternately before either finished.
    let first_done = pending.len().min(
        pending
            .windows(2)
            .position(|w| w[0] == w[1])
            .unwrap_or(pending.len()),
    );
    assert!(first_done >= 4, "{pending:?}");
    assert_eq!(ha.parks, 0, "a slice park reached the handler");
    // Slicing charges b's own budget exactly: as many real fuel parks as
    // when driven synchronously.
    let mut sync = Session::new(&engine).expect("sync");
    sync.set_fuel(Some(25_000));
    let mut hs = Refuel {
        refuel: 25_000,
        parks: 0,
    };
    let p = sync.run(&code, &mut hs).expect("drives");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert!(hs.parks >= 2, "{}", hs.parks);
    assert_eq!(hb.parks, hs.parks);
}
