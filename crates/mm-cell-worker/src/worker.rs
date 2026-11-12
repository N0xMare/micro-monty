//! The worker loop: cell protocol frames in, one engine session ([`CellSession`])
//! driven on this thread, cell protocol frames out.
//!
//! A reader thread decodes host frames into a channel, and turns a `Cancel`
//! for the running cell into a park request at once, so a computing cell
//! stops at its next safepoint. Each engine run goes to its next event:
//! a bound tool call becomes a `NestedCall` and resumes as a future, a wait
//! on futures becomes `Waiting` until the host has delivered every result
//! of the batch, which resumes the cell with all of them and accepts each
//! one once the engine's next event proves it consumed them.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use micro_monty::{
    Completion, HostError, HostErrorKind, HostModule, HostSignature, HostValue, Limits, ParkReason,
    Progress, Resume, Unconverted,
};
use plakous_cell::{
    BindingInput, CELL_PROTOCOL_REVISION, CellOutcome, CellPrelude, CellValue, DeliveryVerdict,
    EngineConfig, HostFrame, LossClass, MAX_CELL_VALUE_DEPTH, MAX_FRAME_BYTES, MontyBootstrap,
    OutputItem, WorkerFrame, json_depth, read_frame_blocking, write_frame_blocking,
};

use crate::{
    ADAPTER, Config, Hooks, Kill, Options, ServeError, codec,
    engine::{CellEngine, CellSession, Park, SessionSettings},
    lock,
};

/// The worker's output, shared by the loop and the print callback.
type Wire = Arc<Mutex<Box<dyn Write + Send>>>;

/// A [`Config`] byte count as this platform's `usize`: one past the
/// address space is no limit.
fn clamp(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// A length as a [`Config`] byte count.
fn bytes(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The terminal of a cancelled cell.
const CANCELLED: &str = "cell cancelled by the host";
/// No cell, in [`Control`] (cell ids count up from zero).
const NO_CELL: u64 = u64::MAX;
/// How long a cell parked by a park request waits for the `Cancel` that
/// asked for it before it continues as if nothing happened.
const CANCEL_GRACE: Duration = Duration::from_millis(100);

/// A loss ends the worker: the frame it sends and the reason.
struct Lost {
    class: LossClass,
    message: String,
}

impl Lost {
    fn protocol(message: impl Into<String>) -> Self {
        Self {
            class: LossClass::ProtocolViolation,
            message: message.into(),
        }
    }

    fn engine(message: impl Into<String>) -> Self {
        Self {
            class: LossClass::EngineFailure,
            message: message.into(),
        }
    }
}

fn write(wire: &Wire, frame: &WorkerFrame) -> Result<(), Lost> {
    write_frame_blocking(&mut *lock(wire), frame).map_err(Lost::engine)
}

fn read(stdin: &mut impl Read) -> Result<Option<HostFrame>, Lost> {
    let Some(body) = read_frame_blocking(stdin, MAX_FRAME_BYTES).map_err(Lost::protocol)? else {
        return Ok(None);
    };
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|_| Lost::protocol("malformed host frame"))
}

/// [`crate::serve`]: the worker from `Hello` to its end.
pub(crate) fn serve(
    mut reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    engine: &dyn CellEngine,
    options: Options,
) -> Result<(), ServeError> {
    let wire: Wire = Arc::new(Mutex::new(writer));
    let hello = WorkerFrame::Hello {
        revision: CELL_PROTOCOL_REVISION.to_owned(),
        engine: micro_monty::ENGINE.to_owned(),
        adapter: ADAPTER.to_owned(),
    };
    if let Err(lost) = write(&wire, &hello) {
        return Err(ServeError::Hello(lost.message));
    }
    let result = configure(&mut reader)
        .and_then(|()| Worker::new(reader, Arc::clone(&wire), engine, options)?.serve());
    result.map_err(|lost| {
        let _ = write(
            &wire,
            &WorkerFrame::HostLost {
                class: lost.class,
                message: lost.message.clone(),
            },
        );
        ServeError::Lost(lost.message)
    })
}

/// Reads `Configure` and checks its protocol revision and limits.
///
/// plak has no micro-monty `EngineConfig` variant yet, so the worker takes
/// the Monty one as a stand-in: both declare the same soft memory limit.
/// SDK stubs ask for a pre-run type check. That check is plak's (HANDOFF
/// scope decision 34) and micro-monty has no checker, so the worker accepts
/// the stubs and runs SDK cells unchecked: a cell a checker would refuse
/// runs, and fails at run time if it fails at all.
fn configure(reader: &mut impl Read) -> Result<(), Lost> {
    match read(reader)? {
        Some(HostFrame::Configure {
            revision, engine, ..
        }) if revision == CELL_PROTOCOL_REVISION && engine == EngineConfig::MONTY => Ok(()),
        Some(HostFrame::Configure { .. }) => {
            Err(Lost::protocol("configure names another revision or limits"))
        }
        Some(_) => Err(Lost::protocol("first parent frame is not configure")),
        None => Err(Lost::protocol("parent closed before configure")),
    }
}

/// What the reader thread hands the loop.
enum Input {
    Frame(HostFrame),
    End,
    Lost(Lost),
}

/// What the reader thread and the loop share about cells.
struct Control {
    /// The cell the latest `StartCell` named, until its terminal.
    current: AtomicU64,
    /// The cell a `Cancel` is stopping, until the loop handles it.
    cancelling: AtomicU64,
    park: Park,
}

impl Control {
    /// Parks the computing cell `cell` until the loop takes the `Cancel`:
    /// one park request, which stays until a cell parks for it (a cell
    /// that has not started yet parks at its first safepoint).
    fn cancel(&self, cell: u64) {
        if cell != self.current.load(Ordering::SeqCst)
            || self.cancelling.swap(cell, Ordering::SeqCst) == cell
        {
            return;
        }
        (self.park)();
    }

    /// `cell` ended.
    fn settle(&self, cell: u64) {
        let _ = self
            .current
            .compare_exchange(cell, NO_CELL, Ordering::SeqCst, Ordering::SeqCst);
        self.settle_cancel(cell);
    }

    /// A `Cancel` of `cell` was handled.
    fn settle_cancel(&self, cell: u64) {
        let _ = self
            .cancelling
            .compare_exchange(cell, NO_CELL, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// Reads host frames on their own thread, so a `Cancel` reaches a cell that
/// is computing: it notes the cell a `StartCell` starts, and turns a
/// `Cancel` of that cell into park requests before it queues the frame.
fn spawn_reader(
    control: Arc<Control>,
    mut reader: Box<dyn Read + Send>,
) -> Result<mpsc::Receiver<Input>, Lost> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("mm-cell-worker-reader".to_owned())
        .spawn(move || {
            loop {
                let input = match read(&mut reader) {
                    Ok(Some(frame)) => {
                        match frame {
                            HostFrame::StartCell { cell, .. } => {
                                control.current.store(cell, Ordering::SeqCst);
                            }
                            HostFrame::Cancel { cell } => control.cancel(cell),
                            _ => {}
                        }
                        Input::Frame(frame)
                    }
                    Ok(None) => Input::End,
                    Err(lost) => Input::Lost(lost),
                };
                let last = !matches!(input, Input::Frame(_));
                if tx.send(input).is_err() || last {
                    break;
                }
            }
        })
        .map_err(|error| Lost::engine(format!("failed to spawn the reader thread: {error}")))?;
    Ok(rx)
}

/// One private host function the bootstrap defined.
struct Binding {
    tool: String,
    input: BindingInput,
}

/// The running cell: the futures it holds, the results the host delivered
/// for them, the batch it waits on, and where the engine stopped.
#[derive(Default)]
struct Running {
    cell: u64,
    futures: BTreeSet<u64>,
    results: BTreeMap<u64, Result<CellValue, String>>,
    /// The batch the engine waits on (`Progress::Waiting`).
    waiting: Option<Vec<u64>>,
    /// The engine is parked (`Progress::Parked`) and resumes with
    /// `Resume::Continue`.
    parked: bool,
    /// It parked for a park request, so the `Cancel` that asked is coming.
    cancel_pending: bool,
    /// The armed yield deadline of an engine-parked cell.
    yield_at: Option<Instant>,
    /// `Yielded` was sent and no `Resume` has come since.
    yielded: bool,
}

/// What prints share with the loop: the cell they belong to, its print
/// budget, acceptances to send before the next event, and a write failure.
struct Out {
    wire: Wire,
    hooks: Arc<dyn Hooks>,
    /// The print budget and its notice ([`Config`]).
    print_limit: usize,
    print_notice: String,
    cell: Option<u64>,
    printed: Printed,
    accepted: Vec<(u64, u64)>,
    failed: Option<String>,
}

impl Out {
    /// Sends the acceptances an engine event has just proved.
    fn flush_accepted(&mut self) {
        if self.accepted.is_empty() {
            return;
        }
        for (cell, call) in std::mem::take(&mut self.accepted) {
            self.send(&WorkerFrame::Delivery {
                cell,
                call,
                verdict: DeliveryVerdict::Accepted,
            });
        }
        self.hooks.boundary("after_accepted");
    }

    /// One piece of output: an event, so it first proves acceptances.
    fn print(&mut self, text: &str) {
        self.flush_accepted();
        let Some(cell) = self.cell else { return };
        if let Some(text) = self
            .printed
            .admit(text, self.print_limit, &self.print_notice)
        {
            self.send(&WorkerFrame::Output {
                cell,
                item: OutputItem::Text { text },
            });
        }
    }

    fn send(&mut self, frame: &WorkerFrame) {
        if self.failed.is_none()
            && let Err(error) = write_frame_blocking(&mut *lock(&self.wire), frame)
        {
            self.failed = Some(error);
        }
    }
}

#[derive(Default)]
struct Printed {
    bytes: usize,
    truncated: bool,
}

impl Printed {
    /// The part of `text` the cell's print budget (`limit` bytes, notice
    /// included) admits, with `notice` the first time the budget runs out.
    fn admit(&mut self, text: &str, limit: usize, notice: &str) -> Option<String> {
        if self.truncated {
            return None;
        }
        let room = limit
            .saturating_sub(notice.len())
            .saturating_sub(self.bytes);
        let mut end = text.len().min(room);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let mut admitted = text[..end].to_owned();
        if end < text.len() {
            admitted.push_str(notice);
            self.truncated = true;
        }
        self.bytes += end;
        (!admitted.is_empty()).then_some(admitted)
    }
}

struct Worker {
    config: Config,
    hooks: Arc<dyn Hooks>,
    kill: Kill,
    wire: Wire,
    session: Box<dyn CellSession>,
    out: Arc<Mutex<Out>>,
    bindings: Option<HashMap<String, Binding>>,
    running: Option<Running>,
    inbox: mpsc::Receiver<Input>,
    control: Arc<Control>,
}

/// The OS host functions a cell may not call, and their `RuntimeError`.
/// The builtins module: the refused OS operations and the tool
/// functions `tools` (untyped; a repeated name is defined once).
fn builtins<'a>(tools: impl IntoIterator<Item = &'a str>) -> HostModule {
    let mut functions: Vec<HostSignature> = Vec::new();
    for name in REFUSED.iter().map(|(n, _)| *n).chain(tools) {
        if !functions.iter().any(|f| f.name == name) {
            functions.push(HostSignature::any(name));
        }
    }
    HostModule {
        functions,
        ..HostModule::default()
    }
}

const REFUSED: [(&str, &str); 4] = [
    ("os.urandom", "os.urandom is not available in cells"),
    ("time.sleep", "time.sleep is not available in cells"),
    ("asyncio.sleep", "asyncio.sleep is not available in cells"),
    (
        "Path.scan",
        "filesystem access is not available in cells; use the nested tools",
    ),
];

impl Worker {
    fn new(
        reader: Box<dyn Read + Send>,
        wire: Wire,
        engine: &dyn CellEngine,
        options: Options,
    ) -> Result<Self, Lost> {
        let Options {
            config,
            hooks,
            kill,
        } = options;
        let EngineConfig::Monty { memory_bytes } = EngineConfig::MONTY else {
            return Err(Lost::engine("the Monty stand-in names no memory limit"));
        };
        // py-ptc renders a final int past 4300 digits as `<int with N bits>`
        // (plak's `format_monty`), in bounded time; every engine does.
        let mut session = engine
            .session(&SessionSettings {
                limits: Limits::new().with_memory(memory_bytes),
                text_limit: config
                    .max_result_bytes
                    .saturating_sub(bytes(config.result_notice.len())),
                host_args: codec::host_args(config.max_value_depth),
            })
            .map_err(|error| Lost::engine(format!("the engine did not start: {error}")))?;
        // OS operations a cell may not perform (plak answers the clock and
        // random in the worker and refuses the rest; docs/spec.md §10).
        session
            .define(&builtins([]))
            .map_err(|error| Lost::engine(format!("the engine did not start: {error}")))?;
        let out = Arc::new(Mutex::new(Out {
            wire: Arc::clone(&wire),
            hooks: Arc::clone(&hooks),
            print_limit: clamp(config.max_print_bytes),
            print_notice: config.print_notice.clone(),
            cell: None,
            printed: Printed::default(),
            accepted: Vec::new(),
            failed: None,
        }));
        let sink = Arc::clone(&out);
        // stdout and stderr merge into one stream in production order.
        session.set_output(Box::new(move |text: &str| {
            lock(&sink).print(text);
        }));
        let control = Arc::new(Control {
            current: AtomicU64::new(NO_CELL),
            cancelling: AtomicU64::new(NO_CELL),
            park: session.park(),
        });
        kill.attach(session.park());
        let inbox = spawn_reader(Arc::clone(&control), reader)?;
        // Every thread of the worker exists now: the binary installs its
        // seccomp filter here (docs/spec.md §10.2).
        hooks
            .confine()
            .map_err(|error| Lost::engine(format!("seccomp: {error}")))?;
        Ok(Self {
            config,
            hooks,
            kill,
            wire,
            session,
            out,
            bindings: None,
            running: None,
            inbox,
            control,
        })
    }

    fn serve(&mut self) -> Result<(), Lost> {
        loop {
            let Some(input) = self.next_input()? else {
                continue;
            };
            match input {
                Input::End => return Ok(()),
                Input::Lost(lost) => return Err(lost),
                Input::Frame(HostFrame::Shutdown) => {
                    return write(
                        &self.wire,
                        &WorkerFrame::ResourceReport {
                            usage: self.hooks.usage(),
                        },
                    );
                }
                Input::Frame(frame) => self.frame(frame)?,
            }
        }
    }

    /// The next host input, or `None` after acting on a timer instead: a
    /// yield window that ran out while the cell waits on the host, or a
    /// park request that no `Cancel` followed.
    fn next_input(&mut self) -> Result<Option<Input>, Lost> {
        let timer = self.running.as_ref().and_then(|running| {
            if running.cancel_pending {
                Some(Instant::now() + CANCEL_GRACE)
            } else if running.yielded {
                None
            } else {
                running.yield_at
            }
        });
        let Some(at) = timer else {
            return Ok(Some(self.inbox.recv().unwrap_or(Input::End)));
        };
        match self
            .inbox
            .recv_timeout(at.saturating_duration_since(Instant::now()))
        {
            Ok(input) => Ok(Some(input)),
            Err(RecvTimeoutError::Disconnected) => Ok(Some(Input::End)),
            Err(RecvTimeoutError::Timeout) => {
                self.timer_fired()?;
                Ok(None)
            }
        }
    }

    fn timer_fired(&mut self) -> Result<(), Lost> {
        let Some(running) = self.running.as_mut() else {
            return Ok(());
        };
        if running.cancel_pending {
            running.cancel_pending = false;
            running.parked = false;
            let progress = self.resume(Resume::Continue)?;
            return self.drive(progress);
        }
        // The window ran out while the cell waits on nested results: it
        // yields there, as an engine-parked cell does at idle.
        running.yield_at = None;
        running.yielded = true;
        write(&self.wire, &WorkerFrame::Yielded { cell: running.cell })
    }

    fn frame(&mut self, frame: HostFrame) -> Result<(), Lost> {
        match frame {
            HostFrame::Reset => {
                if self.running.is_some() {
                    return Err(Lost::protocol("reset while a cell runs"));
                }
                self.session
                    .reset()
                    .map_err(|error| Lost::engine(format!("interpreter reset failed: {error}")))?;
                self.bindings = None;
                Ok(())
            }
            HostFrame::StartCell {
                cell,
                source,
                prelude,
                window,
            } => self.start(cell, &source, prelude, window),
            HostFrame::NestedResult {
                cell,
                call,
                outcome,
            } => self.nested_result(cell, call, outcome),
            HostFrame::Resume { cell, window } => self.resume_window(cell, window),
            HostFrame::Cancel { cell } => self.cancel(cell),
            HostFrame::Configure { .. } => Err(Lost::protocol("repeated configure")),
            HostFrame::Shutdown => Ok(()),
        }
    }

    fn start(
        &mut self,
        cell: u64,
        source: &str,
        prelude: CellPrelude,
        window: Option<u64>,
    ) -> Result<(), Lost> {
        if self.running.is_some() {
            return Err(Lost::protocol("a cell started while another runs"));
        }
        let bootstrap = match prelude {
            CellPrelude::Monty { bootstrap } => bootstrap,
            // plak's client sends an SDK cell as `Reset` and a Monty
            // prelude, so `MontySdk` never arrives; plak's Monty worker
            // refuses it too.
            CellPrelude::MontySdk { .. } | CellPrelude::Quickjs { .. } => {
                return Err(Lost::protocol("a micro-monty cell needs a Monty prelude"));
            }
        };
        self.running = Some(Running {
            cell,
            ..Running::default()
        });
        if let Some(bootstrap) = bootstrap
            && let Err(failure) = self.bootstrap(bootstrap)
        {
            // Its names may be half defined.
            return self.done(CellOutcome::Failed {
                message: failure,
                stored: HashMap::new(),
                generation_lost: true,
            });
        }
        {
            let mut out = lock(&self.out);
            out.cell = Some(cell);
            out.printed = Printed::default();
        }
        if let Some(window) = window {
            self.arm(window);
        }
        let progress = match self
            .session
            .start(source, &self.config.script_name)
            .map_err(Lost::engine)?
        {
            Ok(progress) => progress,
            Err(rendered) => {
                return self.done(CellOutcome::Failed {
                    message: rendered.trim_end().to_owned(),
                    stored: HashMap::new(),
                    generation_lost: false,
                });
            }
        };
        self.drive(progress)
    }

    /// Defines the session's tool functions before its first cell and runs
    /// the bootstrap source; its prints are not part of the cell.
    fn bootstrap(&mut self, bootstrap: MontyBootstrap) -> Result<(), String> {
        const FAILED: &str = "Python tool bootstrap failed: ";
        if self.bindings.is_some() {
            return Err("Python tool bootstrap repeated".into());
        }
        let mut bindings = HashMap::new();
        let names: Vec<String> = bootstrap
            .bindings
            .iter()
            .map(|b| b.function.clone())
            .collect();
        self.session
            .define(&builtins(names.iter().map(String::as_str)))
            .map_err(|error| format!("{FAILED}{}", error.trim_end()))?;
        for binding in bootstrap.bindings {
            bindings.insert(
                binding.function,
                Binding {
                    tool: binding.tool,
                    input: binding.input,
                },
            );
        }
        self.bindings = Some(bindings);
        if bootstrap.source.is_empty() {
            return Ok(());
        }
        let mut progress = self
            .session
            .start(&bootstrap.source, &self.config.script_name)
            .map_err(|error| format!("{FAILED}{error}"))?
            .map_err(|error| format!("{FAILED}{}", error.trim_end()))?;
        loop {
            match progress {
                Progress::Complete(_) => return Ok(()),
                Progress::Error(error) => {
                    return Err(format!("{FAILED}{}", error.traceback.trim_end()));
                }
                // A park request is for the cell; the bootstrap finishes.
                Progress::Parked { .. } => {
                    progress = self
                        .session
                        .resume(Resume::Continue)
                        .map_err(|error| format!("{FAILED}{error}"))?;
                }
                _ => {
                    self.session.cancel();
                    return Err("Python tool bootstrap unexpectedly suspended".into());
                }
            }
        }
    }

    /// Arms the yield deadline `window` milliseconds from now (none if that
    /// is past the clock's range).
    fn arm(&mut self, window: u64) {
        let at = Instant::now().checked_add(Duration::from_millis(window));
        if let Some(running) = self.running.as_mut() {
            running.yield_at = at;
        }
        self.session.set_deadline(at);
    }

    fn resume(&mut self, answer: Resume) -> Result<Progress, Lost> {
        self.session.resume(answer).map_err(Lost::engine)
    }

    /// Sends the acceptances an engine event proved and checks that every
    /// output frame was written.
    fn flush_out(&self) -> Result<(), Lost> {
        let mut out = lock(&self.out);
        out.flush_accepted();
        match out.failed.take() {
            Some(error) => Err(Lost::engine(error)),
            None => Ok(()),
        }
    }

    /// Runs the cell from `progress` until it waits on the host, parks or
    /// ends.
    fn drive(&mut self, mut progress: Progress) -> Result<(), Lost> {
        loop {
            self.flush_out()?;
            let answer = match progress {
                Progress::HostCall {
                    call,
                    name,
                    args,
                    kwargs,
                    ..
                } => self.host_call(call, &name, &args, &kwargs)?,
                Progress::Waiting { calls } => match self.wait_on(calls)? {
                    Some(batch) => batch,
                    None => return Ok(()),
                },
                // A kill (`Kill::kill`) parks the cell and ends the worker,
                // as killing its process would.
                Progress::Parked { .. } if self.kill.is_killed() => {
                    return Err(Lost::engine("cell worker killed"));
                }
                Progress::Parked {
                    reason: ParkReason::Fuel,
                } => Resume::Continue,
                // The request of a `Cancel` whose cell ended before it
                // parked: nothing waits for this cell.
                Progress::Parked {
                    reason: ParkReason::Requested,
                } if self.running.as_ref().is_some_and(|running| {
                    self.control.cancelling.load(Ordering::SeqCst) != running.cell
                }) =>
                {
                    Resume::Continue
                }
                Progress::Parked { reason } => return self.parked(reason),
                Progress::Complete(completion) => {
                    return self.done(CellOutcome::Completed {
                        value: Some(render(completion, &self.config.result_notice)),
                        stored: HashMap::new(),
                    });
                }
                Progress::Error(error) => {
                    return self.done(CellOutcome::Failed {
                        message: error.traceback.trim_end().to_owned(),
                        stored: HashMap::new(),
                        generation_lost: false,
                    });
                }
                _ => return Err(Lost::engine("the engine stopped for an unknown reason")),
            };
            progress = self.resume(answer)?;
        }
    }

    fn parked(&mut self, reason: ParkReason) -> Result<(), Lost> {
        let running = self
            .running
            .as_mut()
            .ok_or_else(|| Lost::protocol("a park without a running cell"))?;
        running.parked = true;
        if reason == ParkReason::Requested {
            running.cancel_pending = true;
            return Ok(());
        }
        // The yield deadline: a cell that already yielded while it waited
        // on the host stays yielded.
        running.yield_at = None;
        if running.yielded {
            return Ok(());
        }
        running.yielded = true;
        write(&self.wire, &WorkerFrame::Yielded { cell: running.cell })
    }

    /// Answers one tool call of the cell: a bound tool with a valid argument
    /// becomes a nested call and a future; anything else raises in Python.
    fn host_call(
        &mut self,
        call: u64,
        name: &str,
        args: &[HostValue],
        kwargs: &[(String, HostValue)],
    ) -> Result<Resume, Lost> {
        let raise = |kind, message: String| {
            Ok(Resume::Error(HostError {
                kind,
                message,
                data: None,
                class: None,
            }))
        };
        if let Some((_, message)) = REFUSED.iter().find(|(refused, _)| *refused == name) {
            return raise(HostErrorKind::RuntimeError, (*message).to_owned());
        }
        let Some(binding) = self
            .bindings
            .as_ref()
            .and_then(|bindings| bindings.get(name))
        else {
            return raise(
                HostErrorKind::NameError,
                format!("name '{name}' is not defined"),
            );
        };
        let argument = match (args, kwargs) {
            ([argument], []) if accepts(binding.input, argument) => argument,
            _ => {
                let expected = match binding.input {
                    BindingInput::Dictionary => "dictionary",
                    BindingInput::String => "string",
                };
                return raise(
                    HostErrorKind::TypeError,
                    format!("Python tool requires exactly one {expected} argument"),
                );
            }
        };
        let running = self
            .running
            .as_mut()
            .ok_or_else(|| Lost::protocol("a tool call without a running cell"))?;
        if running.futures.len() >= self.config.max_outstanding_nested {
            return raise(
                HostErrorKind::RuntimeError,
                self.config.outstanding_nested_error.clone(),
            );
        }
        let input = match codec::call_to_json(argument, self.config.max_value_depth) {
            Ok(input) if json_depth(&input) > MAX_CELL_VALUE_DEPTH => {
                let reason =
                    format!("Python tool argument nests deeper than {MAX_CELL_VALUE_DEPTH} levels");
                return raise(HostErrorKind::ValueError, reason);
            }
            Ok(input) => input,
            Err(reason) => return raise(HostErrorKind::ValueError, reason),
        };
        write(
            &self.wire,
            &WorkerFrame::NestedCall {
                cell: running.cell,
                call,
                name: binding.tool.clone(),
                input: CellValue::from(input),
            },
        )?;
        running.futures.insert(call);
        Ok(Resume::Future)
    }

    /// The cell waits on `calls`: tell the host, and return the batch at
    /// once if every result is already here.
    fn wait_on(&mut self, calls: Vec<u64>) -> Result<Option<Resume>, Lost> {
        let running = self
            .running
            .as_mut()
            .ok_or_else(|| Lost::protocol("a wait without a running cell"))?;
        if let Some(unknown) = calls.iter().find(|id| !running.futures.contains(id)) {
            return Err(Lost::protocol(format!(
                "micro-monty requested unknown nested call id {unknown}"
            )));
        }
        write(
            &self.wire,
            &WorkerFrame::Waiting {
                cell: running.cell,
                calls: calls.clone(),
            },
        )?;
        running.waiting = Some(calls);
        self.complete_batch()
    }

    /// The batch the cell waits on, once the host delivered every result of
    /// it; its results are accepted at the engine's next event.
    fn complete_batch(&mut self) -> Result<Option<Resume>, Lost> {
        let Some(running) = self.running.as_mut() else {
            return Ok(None);
        };
        let Some(pending) = running.waiting.as_ref() else {
            return Ok(None);
        };
        if !pending.iter().all(|id| running.results.contains_key(id)) {
            return Ok(None);
        }
        let pending = running.waiting.take().unwrap_or_default();
        let mut batch = Vec::with_capacity(pending.len());
        for id in &pending {
            let Some(result) = running.results.remove(id) else {
                return Err(Lost::protocol(format!(
                    "micro-monty nested reply missing for call {id}"
                )));
            };
            running.futures.remove(id);
            match codec::nested_outcome(
                result,
                self.config.max_value_depth,
                clamp(self.config.max_nested_error_bytes),
            ) {
                Ok(result) => batch.push((*id, result)),
                Err(reason) => {
                    // Nothing of the batch reaches the interpreter: every
                    // result the host delivered is definitely unobserved.
                    let unobserved = pending
                        .iter()
                        .chain(running.results.keys())
                        .copied()
                        .collect::<BTreeSet<_>>();
                    for call in unobserved {
                        write(
                            &self.wire,
                            &WorkerFrame::Delivery {
                                cell: running.cell,
                                call,
                                verdict: DeliveryVerdict::NotObserved(
                                    "the batch carrying the result never reached the interpreter"
                                        .into(),
                                ),
                            },
                        )?;
                    }
                    return Err(Lost::protocol(format!(
                        "micro-monty nested reply for call {id}: {reason}"
                    )));
                }
            }
        }
        let cell = running.cell;
        lock(&self.out)
            .accepted
            .extend(pending.iter().map(|id| (cell, *id)));
        self.hooks.boundary("before_delivery");
        Ok(Some(Resume::Batch(batch)))
    }

    #[allow(
        clippy::similar_names,
        reason = "cell and call are the protocol's own field names"
    )]
    fn nested_result(
        &mut self,
        cell: u64,
        call: u64,
        outcome: Result<CellValue, String>,
    ) -> Result<(), Lost> {
        let verdict = |verdict| WorkerFrame::Delivery {
            cell,
            call,
            verdict,
        };
        let Some(running) = self.running.as_mut() else {
            return write(
                &self.wire,
                &verdict(DeliveryVerdict::NotObserved(
                    "the cell had already ended when its result was delivered".into(),
                )),
            );
        };
        if running.cell != cell {
            return write(
                &self.wire,
                &verdict(DeliveryVerdict::Refused(
                    "the result names another cell".into(),
                )),
            );
        }
        if !running.futures.contains(&call) || running.results.contains_key(&call) {
            return write(
                &self.wire,
                &verdict(DeliveryVerdict::Refused(
                    "the result names no pending call of the cell".into(),
                )),
            );
        }
        running.results.insert(call, outcome);
        match self.complete_batch()? {
            Some(batch) => {
                let progress = self.resume(batch)?;
                self.drive(progress)
            }
            None => Ok(()),
        }
    }

    /// `Resume` of an engine-parked yield: a new window, and the cell
    /// continues if it parked. A cell that yielded while it waited on the
    /// host keeps waiting under the new window; a stale cell id is ignored.
    fn resume_window(&mut self, cell: u64, window: u64) -> Result<(), Lost> {
        let Some(running) = self.running.as_mut().filter(|running| running.cell == cell) else {
            return Ok(());
        };
        running.yielded = false;
        let parked = running.parked && !running.cancel_pending;
        self.arm(window);
        if !parked {
            return Ok(());
        }
        if let Some(running) = self.running.as_mut() {
            running.parked = false;
        }
        let progress = self.resume(Resume::Continue)?;
        self.drive(progress)
    }

    /// Ends the running cell where it stands: its frames, tasks and futures
    /// are dropped and globals keep what it assigned. The worker lives on.
    fn cancel(&mut self, cell: u64) -> Result<(), Lost> {
        if self
            .running
            .as_ref()
            .is_none_or(|running| running.cell != cell)
        {
            self.control.settle_cancel(cell);
            return Ok(());
        }
        self.session.cancel();
        self.done(CellOutcome::Failed {
            message: CANCELLED.to_owned(),
            stored: HashMap::new(),
            generation_lost: false,
        })
    }

    /// Sends the cell's terminal, declining the results the interpreter
    /// never consumed.
    fn done(&mut self, outcome: CellOutcome) -> Result<(), Lost> {
        let running = self
            .running
            .take()
            .ok_or_else(|| Lost::protocol("a terminal without a running cell"))?;
        self.control.settle(running.cell);
        self.session.set_deadline(None);
        self.flush_out()?;
        lock(&self.out).cell = None;
        for call in running.results.keys() {
            write(
                &self.wire,
                &WorkerFrame::Delivery {
                    cell: running.cell,
                    call: *call,
                    verdict: DeliveryVerdict::NotObserved(
                        "the cell ended before the interpreter observed the result".into(),
                    ),
                },
            )?;
        }
        write(
            &self.wire,
            &WorkerFrame::CellDone {
                cell: running.cell,
                outcome,
            },
        )
    }
}

/// Whether `argument` has the type a binding takes. An argument the engine
/// stopped converting for size still names its type.
fn accepts(input: BindingInput, argument: &HostValue) -> bool {
    let expected = match input {
        BindingInput::Dictionary => "dict",
        BindingInput::String => "str",
    };
    match argument {
        HostValue::Dict(_) => input == BindingInput::Dictionary,
        HostValue::Str(_) => input == BindingInput::String,
        HostValue::Unconverted(Unconverted::TooLarge { type_name }) => type_name == expected,
        _ => false,
    }
}

/// A cell's final value as Python `str()` renders it, `None` included,
/// within the result bound, with `notice` when it was cut.
fn render(completion: Option<Completion>, notice: &str) -> String {
    let Some(completion) = completion else {
        return "None".to_owned();
    };
    let mut text = completion.str;
    if completion.str_truncated {
        text.push_str(notice);
    }
    text
}
