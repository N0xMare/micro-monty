//! Deterministic crash cuts for plak's delivery tests, as plak's Monty
//! worker has them; absent from ordinary builds.

/// `--test-cell-hooks` asks whether this build has the cuts.
pub(crate) fn probe() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--test-cell-hooks") {
        return false;
    }
    println!("cell crash hooks enabled");
    true
}

use std::{fs::File, io::Write as _, sync::OnceLock};

/// `PLAK_TEST_CELL_CUT_READY`, opened before the seccomp filter forbids
/// opening files.
static READY: OnceLock<File> = OnceLock::new();

/// Opens the cut's ready file (empty until the cut) and returns its
/// descriptor, which the seccomp filter lets the worker write.
pub(crate) fn prepare() -> Vec<u32> {
    use std::os::unix::io::AsRawFd;
    if std::env::var_os("PLAK_TEST_CELL_CUT").is_none() {
        return Vec::new();
    }
    let Ok(path) = std::env::var("PLAK_TEST_CELL_CUT_READY") else {
        return Vec::new();
    };
    let Ok(file) = File::create(path) else {
        std::process::exit(5);
    };
    let fd = u32::try_from(file.as_raw_fd()).unwrap_or(u32::MAX);
    let _ = READY.set(file);
    vec![fd]
}

/// Stops the worker for good at the cut `PLAK_TEST_CELL_CUT` names, after
/// writing the cut's name to `PLAK_TEST_CELL_CUT_READY`.
pub(crate) fn boundary(name: &str) {
    if std::env::var("PLAK_TEST_CELL_CUT").as_deref() == Ok(name) {
        let written = READY
            .get()
            .map(|mut ready| ready.write_all(name.as_bytes()));
        if !matches!(written, Some(Ok(()))) {
            std::process::exit(5);
        }
        loop {
            std::thread::park();
        }
    }
}
