//! `cell-tracer LOG_DIR WORKER`: runs `WORKER` (a plak Python cell worker)
//! and forwards plakous-cell-protocol.v2 frames (u32 big-endian length, then
//! JSON) unchanged between plak and it, logging every frame with a monotonic
//! timestamp to `LOG_DIR/trace-<unix ns>-<pid>.jsonl`.
//!
//! Around each `StartCell` and `CellDone` it also logs a probe of the worker
//! process: RSS and peak RSS from `/proc/PID/status`, and CPU time from the
//! sum of `/proc/PID/task/*/schedstat` (nanoseconds). plak clears the
//! worker's environment and passes no arguments, so point `PLAK_PY_HOST` at
//! a two-line script that `exec`s this with the arguments.
//!
//! Line shapes: `{"t":NS,"d":"h"|"w","f":FRAME}` for host→worker and
//! worker→host frames, `{"t":NS,"d":"p","rss_kb":..,"hwm_kb":..,"cpu_ns":..}`
//! for probes, `{"t":NS,"d":"x","status":..}` when the worker exits.

use std::{
    fs,
    io::{self, BufWriter, Read, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

type Log = Arc<Mutex<BufWriter<fs::File>>>;

fn read_frame(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let mut body = vec![0_u8; u32::from_be_bytes(length) as usize];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

fn probe(pid: u32) -> String {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let mut cpu_ns = 0_u64;
    if let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) {
        for task in tasks.flatten() {
            if let Ok(text) = fs::read_to_string(task.path().join("schedstat")) {
                cpu_ns += text
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }
    }
    format!(
        "\"rss_kb\":{},\"hwm_kb\":{},\"cpu_ns\":{cpu_ns}",
        field("VmRSS:"),
        field("VmHWM:")
    )
}

fn log_line(log: &Log, start: Instant, body: &str) {
    let t = start.elapsed().as_nanos();
    if let Ok(mut log) = log.lock() {
        let _ = writeln!(log, "{{\"t\":{t},{body}}}");
        let _ = log.flush();
    }
}

fn is_frame(frame: &[u8], name: &str) -> bool {
    let needle = format!("\"frame\":\"{name}\"");
    frame
        .windows(needle.len())
        .take(64)
        .any(|window| window == needle.as_bytes())
}

/// Forwards frames from `from` to `to`, logging each with direction `dir`.
fn pump(
    mut from: impl Read,
    mut to: impl Write,
    dir: char,
    log: &Log,
    start: Instant,
    pid: u32,
) -> io::Result<()> {
    while let Some(frame) = read_frame(&mut from)? {
        let starts = dir == 'h' && is_frame(&frame, "start_cell");
        if starts {
            log_line(log, start, &format!("\"d\":\"p\",{}", probe(pid)));
        }
        let text = String::from_utf8_lossy(&frame);
        log_line(log, start, &format!("\"d\":\"{dir}\",\"f\":{text}"));
        if dir == 'w' && is_frame(&frame, "cell_done") {
            log_line(log, start, &format!("\"d\":\"p\",{}", probe(pid)));
        }
        let length = u32::try_from(frame.len()).map_err(io::Error::other)?;
        to.write_all(&length.to_be_bytes())?;
        to.write_all(&frame)?;
        to.flush()?;
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let (Some(dir), Some(worker)) = (args.next(), args.next()) else {
        eprintln!("usage: cell-tracer LOG_DIR WORKER");
        std::process::exit(2);
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = std::path::Path::new(&dir).join(format!("trace-{stamp}-{}.jsonl", std::process::id()));
    let file = match fs::File::create(&path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("cell-tracer: {}: {error}", path.display());
            std::process::exit(2);
        }
    };
    let log: Log = Arc::new(Mutex::new(BufWriter::new(file)));
    let start = Instant::now();
    let mut child = match Command::new(&worker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            eprintln!("cell-tracer: spawn: {error}");
            std::process::exit(2);
        }
    };
    let pid = child.id();
    log_line(
        &log,
        start,
        &format!(
            "\"d\":\"s\",\"worker\":{:?},\"unix_ns\":{stamp}",
            worker.to_string_lossy()
        ),
    );
    let (Some(child_in), Some(child_out)) = (child.stdin.take(), child.stdout.take()) else {
        std::process::exit(2);
    };
    let up_log = Arc::clone(&log);
    let up = thread::spawn(move || {
        let _ = pump(io::stdin().lock(), child_in, 'h', &up_log, start, pid);
        // Dropping `child_in` closes the worker's stdin, as plak's EOF would.
    });
    let _ = pump(child_out, io::stdout().lock(), 'w', &log, start, pid);
    let status = child.wait().map_or(-1, |s| s.code().unwrap_or(-1));
    log_line(&log, start, &format!("\"d\":\"x\",\"status\":{status}"));
    drop(up);
    std::process::exit(if status < 0 { 1 } else { status });
}
