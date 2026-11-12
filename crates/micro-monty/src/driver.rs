//! An optional convenience driver over the event loop ([Driving a
//! session](crate#driving-a-session)): a [`HostHandler`] (or [`AsyncHostHandler`])
//! answers host calls, waits and parks, and [`Session::drive`] (or
//! [`Session::drive_async`], which runs the cell in fuel slices and yields to
//! the executor between them) resumes the session until the cell ends or the
//! handler stops. [`Session::start`] and [`Session::resume`] stay the core
//! API; the driver only calls them.

use crate::{Code, HostValue, ParkReason, Progress, Resume, ResumeError, Session};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// A host call, as [`Progress::HostCall`] reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Call<'a> {
    /// The call id.
    pub call: u64,
    /// The function's id, as [`Session::define`] returned it.
    pub function: u32,
    /// The function's call name.
    pub name: &'a str,
    /// Positional arguments.
    pub args: &'a [HostValue],
    /// Keyword arguments, in call order.
    pub kwargs: &'a [(String, HostValue)],
}

/// What a handler does at a stop.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Step {
    /// Resume the session with this answer.
    Resume(Resume),
    /// Return the stop to the caller of [`Session::drive`] as it is.
    Stop,
}

/// Answers a session's stops for [`Session::drive`]. Each method gets the
/// session (stopped: it may snapshot it, refuel it, read its ids) and
/// returns what to do. Only `host_call` is required; waits and parks stop
/// by default.
pub trait HostHandler {
    /// A host call: usually `Step::Resume(Resume::Value(..))`,
    /// `Resume::Error(..)` or `Resume::Future`.
    fn host_call(&mut self, session: &mut Session, call: Call<'_>) -> Step;

    /// Every task waits on these host futures: usually
    /// `Step::Resume(Resume::Batch(..))` with one or more results.
    fn waiting(&mut self, session: &mut Session, calls: &[u64]) -> Step {
        let _ = (session, calls);
        Step::Stop
    }

    /// A park (fuel, deadline or a [`crate::ParkHandle`] request):
    /// `Step::Resume(Resume::Continue)` goes on, after
    /// [`Session::set_fuel`] for a fuel park.
    fn parked(&mut self, session: &mut Session, reason: ParkReason) -> Step {
        let _ = (session, reason);
        Step::Stop
    }
}

/// A boxed, `Send` future, as [`AsyncHostHandler`] methods return.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// [`HostHandler`] for async hosts ([`Session::drive_async`]): each answer
/// may await (a tool call, a channel, a timer) on the host's executor.
pub trait AsyncHostHandler: Send {
    /// See [`HostHandler::host_call`].
    fn host_call<'a>(&'a mut self, session: &'a mut Session, call: Call<'a>)
    -> BoxFuture<'a, Step>;

    /// See [`HostHandler::waiting`].
    fn waiting<'a>(
        &'a mut self,
        session: &'a mut Session,
        calls: &'a [u64],
    ) -> BoxFuture<'a, Step> {
        let _ = (session, calls);
        Box::pin(async { Step::Stop })
    }

    /// See [`HostHandler::parked`].
    fn parked<'a>(
        &'a mut self,
        session: &'a mut Session,
        reason: ParkReason,
    ) -> BoxFuture<'a, Step> {
        let _ = (session, reason);
        Box::pin(async { Step::Stop })
    }
}

/// The handler's input for `progress`, or `None` for a final one.
enum Stop<'a> {
    Call(Call<'a>),
    Waiting(&'a [u64]),
    Parked(ParkReason),
}

fn stop_of(progress: &Progress) -> Option<Stop<'_>> {
    match progress {
        Progress::HostCall {
            call,
            function,
            name,
            args,
            kwargs,
        } => Some(Stop::Call(Call {
            call: *call,
            function: *function,
            name,
            args,
            kwargs,
        })),
        Progress::Waiting { calls } => Some(Stop::Waiting(calls)),
        Progress::Parked { reason } => Some(Stop::Parked(*reason)),
        _ => None,
    }
}

impl Session {
    /// Starts `code` and [drives](Session::drive) it.
    ///
    /// # Errors
    ///
    /// As [`Session::drive`].
    pub fn run(
        &mut self,
        code: &Code,
        handler: &mut dyn HostHandler,
    ) -> Result<Progress, ResumeError> {
        let progress = self.start(code);
        self.drive(progress, handler)
    }

    /// Answers stops with `handler` until the cell completes or raises, or
    /// the handler returns [`Step::Stop`]; returns that progress. The
    /// session can then be resumed by hand, or driven again.
    ///
    /// # Errors
    ///
    /// A [`ResumeError`] if the handler's answer does not fit the stop
    /// (the session is unchanged and still stopped there).
    pub fn drive(
        &mut self,
        mut progress: Progress,
        handler: &mut dyn HostHandler,
    ) -> Result<Progress, ResumeError> {
        loop {
            let step = match stop_of(&progress) {
                None => return Ok(progress),
                Some(Stop::Call(call)) => handler.host_call(self, call),
                Some(Stop::Waiting(calls)) => handler.waiting(self, calls),
                Some(Stop::Parked(reason)) => handler.parked(self, reason),
            };
            match step {
                Step::Resume(resume) => progress = self.resume(resume)?,
                Step::Stop => return Ok(progress),
            }
        }
    }

    /// Starts `code` in fuel slices and [drives](Session::drive_async) it.
    ///
    /// # Errors
    ///
    /// As [`Session::drive`].
    pub async fn run_async(
        &mut self,
        code: &Code,
        handler: &mut dyn AsyncHostHandler,
    ) -> Result<Progress, ResumeError> {
        let progress = self.sliced(Run::Start(code)).await?;
        self.drive_async(progress, handler).await
    }

    /// [`Session::drive`] with an [`AsyncHostHandler`], for any executor.
    /// The cell runs in slices of [`ASYNC_SLICE_FUEL`] fuel units: when a
    /// slice runs out, the future wakes itself and returns `Pending`, and
    /// the next poll continues the cell where it stopped (pause-anywhere),
    /// so other tasks on the same thread run in between. Slicing does not
    /// change the session's own fuel budget: it is charged exactly, and
    /// only its exhaustion reaches [`AsyncHostHandler::parked`]. A host
    /// that wants the cell on a CPU thread of its own can still wrap this
    /// in its executor's `spawn_blocking`.
    ///
    /// # Errors
    ///
    /// As [`Session::drive`].
    pub async fn drive_async(
        &mut self,
        mut progress: Progress,
        handler: &mut dyn AsyncHostHandler,
    ) -> Result<Progress, ResumeError> {
        loop {
            let step = match stop_of(&progress) {
                None => return Ok(progress),
                Some(Stop::Call(call)) => handler.host_call(self, call).await,
                Some(Stop::Waiting(calls)) => handler.waiting(self, calls).await,
                Some(Stop::Parked(reason)) => handler.parked(self, reason).await,
            };
            match step {
                Step::Resume(resume) => progress = self.sliced(Run::Resume(resume)).await?,
                Step::Stop => return Ok(progress),
            }
        }
    }

    /// Runs `first`, then continues across slice parks (yielding to the
    /// executor at each), until a stop for the handler.
    async fn sliced(&mut self, first: Run<'_>) -> Result<Progress, ResumeError> {
        let mut next = first;
        if let Run::Start(_) = next {
            // The cell's own budget, which the slices then divide.
            self.refuel();
        }
        loop {
            // The session's budget, exact, and the slice under it.
            let (budget, _) = self.vm.fuel_account();
            let slice = budget.map_or(ASYNC_SLICE_FUEL, |f| f.min(ASYNC_SLICE_FUEL));
            self.vm.set_fuel(Some(slice));
            // A collection step ends the slice too (`Session::set_gc_yield`).
            let yielding = core::mem::replace(&mut self.vm.gc_yield, true);
            let result = match next {
                Run::Start(code) => Ok(self.start_cell(code, false)),
                Run::Resume(resume) => self.resume(resume),
            };
            self.vm.gc_yield = yielding;
            let (left, overdraft) = self.vm.fuel_account();
            let used = slice
                .saturating_sub(left.unwrap_or(0))
                .saturating_add(overdraft);
            let budget = budget.map(|f| f.saturating_sub(used));
            self.vm.set_fuel(budget);
            match result? {
                // Not when the next operation wants more than the budget
                // left (`Session::fuel_wanted`): resuming would run it.
                Progress::Parked {
                    reason: ParkReason::Fuel,
                } if budget != Some(0) && budget.is_none_or(|b| self.vm.fuel_wanted() <= b) => {
                    YieldNow(false).await;
                    next = Run::Resume(Resume::Continue);
                }
                progress => return Ok(progress),
            }
        }
    }
}

/// Fuel units [`Session::drive_async`] runs a cell for between yields to
/// the executor: up to about 2.5 ms of interpretation natively \[E\],
/// plus at most one native step.
pub const ASYNC_SLICE_FUEL: u64 = 10_000;

/// What [`Session::sliced`] runs first.
enum Run<'a> {
    Start(&'a Code),
    Resume(Resume),
}

/// Returns `Pending` once, waking itself, then `Ready`.
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
