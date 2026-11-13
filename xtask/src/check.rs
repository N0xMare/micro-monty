//! The stage runner of `cargo xtask check` (the stages are in `main.rs`).
//!
//! A stage is one or more commands run in order. It starts once the stages
//! it comes `after` have passed, so independent stages run at the same
//! time; its output goes to `target/check/logs/<stage>.log`. A stage
//! marked `alone` (a wall-clock measurement) starts only after every other
//! stage has passed, with the machine to itself. The first failure stops
//! every running stage (each process the gate started), prints the end of
//! the failing stage's log and fails the gate. Stages run at `nice` 10
//! unless marked `urgent`: the ones on the critical path keep their speed
//! and the others take the cores they leave. The debug tests are urgent
//! too, so their wall bounds do not starve.
//!
//! `--serial` runs one stage at a time, in list order, with the output on
//! the terminal.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{BufRead, BufReader, Write as _},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// One node of the gate graph.
pub(crate) struct Stage {
    pub(crate) name: &'static str,
    /// Stages that must pass before this one starts.
    after: Vec<&'static str>,
    alone: bool,
    urgent: bool,
    commands: Vec<Command>,
}

impl Stage {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            after: Vec::new(),
            alone: false,
            urgent: false,
            commands: Vec::new(),
        }
    }

    pub(crate) fn after(mut self, stages: &[&'static str]) -> Self {
        self.after.extend_from_slice(stages);
        self
    }

    /// Runs once every other stage has passed, and nothing runs beside it.
    pub(crate) fn alone(mut self) -> Self {
        self.alone = true;
        self
    }

    /// Runs at normal priority beside the others, which run at `nice` 10.
    pub(crate) fn urgent(mut self) -> Self {
        self.urgent = true;
        self
    }

    pub(crate) fn run(mut self, command: Command) -> Self {
        self.commands.push(command);
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Waiting,
    Running,
    Passed,
    Failed,
    Stopped,
}

/// Runs `stages` (at the same time unless `serial`); fails if one fails.
#[allow(clippy::too_many_lines, reason = "the scheduling loop and its report")]
pub(crate) fn run(stages: Vec<Stage>, root: &Path, serial: bool) -> Result<(), String> {
    let names: Vec<&'static str> = stages.iter().map(|s| s.name).collect();
    let mut after = Vec::new();
    for stage in &stages {
        let mut deps = Vec::new();
        for dep in &stage.after {
            deps.push(names.iter().position(|n| n == dep).ok_or_else(|| {
                format!(
                    "check: stage {} comes after unknown stage {dep}",
                    stage.name
                )
            })?);
        }
        after.push(deps);
    }
    let alone: Vec<bool> = stages.iter().map(|s| s.alone).collect();
    let niced: Vec<bool> = stages
        .iter()
        .map(|s| !serial && !s.alone && !s.urgent)
        .collect();
    let mut pending: Vec<Option<Vec<Command>>> =
        stages.into_iter().map(|s| Some(s.commands)).collect();
    let logs = root.join("target/check/logs");
    fs::create_dir_all(&logs).map_err(|e| format!("cannot create {}: {e}", logs.display()))?;
    let began = Instant::now();
    let mut state = vec![State::Waiting; names.len()];
    let mut times = vec![Duration::ZERO; names.len()];
    let (done, finished) = mpsc::channel::<(usize, Result<(), String>, Duration)>();
    let mut failure: Option<(usize, String)> = None;
    loop {
        if failure.is_none() {
            for i in 0..names.len() {
                let running = state.contains(&State::Running);
                if running
                    && (serial || (0..names.len()).any(|j| alone[j] && state[j] == State::Running))
                {
                    break;
                }
                let others_passed =
                    (0..names.len()).all(|j| j == i || alone[j] || state[j] == State::Passed);
                let ready = state[i] == State::Waiting
                    && after[i].iter().all(|&d| state[d] == State::Passed)
                    && (!alone[i] || (!running && others_passed));
                if !ready {
                    continue;
                }
                state[i] = State::Running;
                let commands = pending[i].take().unwrap_or_default();
                let log = (!serial).then(|| logs.join(format!("{}.log", names[i])));
                let (done, name, nice) = (done.clone(), names[i], niced[i]);
                println!(
                    "check: {name} started ({} since start)",
                    clock(began.elapsed())
                );
                thread::spawn(move || {
                    let start = Instant::now();
                    let result = run_stage(commands, log.as_deref(), nice);
                    let _ = done.send((i, result, start.elapsed()));
                });
            }
        }
        if !state.contains(&State::Running) {
            break;
        }
        let Ok((i, result, took)) = finished.recv() else {
            break;
        };
        times[i] = took;
        match result {
            Ok(()) => {
                state[i] = State::Passed;
                println!(
                    "check: {} passed in {} ({} since start)",
                    names[i],
                    clock(took),
                    clock(began.elapsed())
                );
            }
            Err(_) if failure.is_some() => state[i] = State::Stopped,
            Err(error) => {
                state[i] = State::Failed;
                println!("check: {} FAILED after {}", names[i], clock(took));
                failure = Some((i, error));
                stop_descendants();
            }
        }
    }
    println!("check: stage wall times");
    for (i, name) in names.iter().enumerate() {
        let what = match state[i] {
            State::Passed => clock(times[i]),
            State::Failed => format!("{} FAILED", clock(times[i])),
            State::Stopped => format!("{} stopped", clock(times[i])),
            State::Waiting | State::Running => "not run".to_owned(),
        };
        println!("check:   {name:<20} {what}");
    }
    println!("check: {} in all", clock(began.elapsed()));
    match failure {
        None => {
            println!("check: every gate passed");
            Ok(())
        }
        Some((i, error)) if serial => Err(format!("{}: {error}", names[i])),
        Some((i, error)) => {
            let log = logs.join(format!("{}.log", names[i]));
            print_tail(&log, 60);
            Err(format!("{}: {error} (log: {})", names[i], log.display()))
        }
    }
}

/// Runs one stage's commands in order, its output to `log` if given.
fn run_stage(commands: Vec<Command>, log: Option<&Path>, nice: bool) -> Result<(), String> {
    let file = log
        .map(|log| File::create(log).map_err(|e| format!("cannot create {}: {e}", log.display())))
        .transpose()?;
    for command in commands {
        let shown = shown(&command);
        let mut command = if nice { niced(&command) } else { command };
        if let Some(mut file) = file.as_ref() {
            let _ = writeln!(file, "check: {shown}");
            let out = file.try_clone().map_err(|e| e.to_string())?;
            let err = file.try_clone().map_err(|e| e.to_string())?;
            command.stdout(Stdio::from(out)).stderr(Stdio::from(err));
        } else {
            println!("check: {shown}");
        }
        let status = command
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("cannot run {shown}: {e}"))?;
        if !status.success() {
            return Err(format!("gate failed: {shown}"));
        }
    }
    Ok(())
}

/// `command` under `nice -n 10`, with its directory and environment.
fn niced(command: &Command) -> Command {
    let mut niced = Command::new("nice");
    niced
        .args(["-n", "10"])
        .arg(command.get_program())
        .args(command.get_args());
    if let Some(dir) = command.get_current_dir() {
        niced.current_dir(dir);
    }
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => niced.env(key, value),
            None => niced.env_remove(key),
        };
    }
    niced
}

/// Sends SIGTERM to every process this one started, however deep (the
/// stages' cargo, test binaries, oracles and `mm` processes), a few times
/// so that workers starting a next process between two passes stop too.
fn stop_descendants() {
    let me = std::process::id();
    for _ in 0..5 {
        let Ok(out) = Command::new("ps").args(["-A", "-o", "pid=,ppid="]).output() else {
            return;
        };
        let pairs: Vec<(u32, u32)> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let mut it = line.split_whitespace().map(str::parse::<u32>);
                Some((it.next()?.ok()?, it.next()?.ok()?))
            })
            .collect();
        let mut tree = BTreeSet::from([me]);
        let mut grew = true;
        while grew {
            grew = false;
            for &(pid, ppid) in &pairs {
                if tree.contains(&ppid) && tree.insert(pid) {
                    grew = true;
                }
            }
        }
        tree.remove(&me);
        if tree.is_empty() {
            return;
        }
        let _ = Command::new("kill")
            .arg("-TERM")
            .args(tree.iter().map(u32::to_string))
            .stderr(Stdio::null())
            .status();
        thread::sleep(Duration::from_millis(500));
    }
}

fn print_tail(log: &Path, lines: usize) {
    let Ok(file) = File::open(log) else {
        return;
    };
    let all: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
    println!("check: the last {lines} lines of {}:", log.display());
    for line in &all[all.len().saturating_sub(lines)..] {
        println!("{line}");
    }
}

/// A command as a shell would show it, with the environment it sets.
fn shown(command: &Command) -> String {
    let mut parts: Vec<String> = command
        .get_envs()
        .filter_map(|(k, v)| Some(format!("{}={}", k.to_str()?, v?.to_str()?)))
        .collect();
    let program = PathBuf::from(command.get_program());
    parts.push(program.file_name().map_or_else(
        || program.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    ));
    parts.extend(command.get_args().map(|a| a.to_string_lossy().into_owned()));
    parts.join(" ")
}

fn clock(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}
