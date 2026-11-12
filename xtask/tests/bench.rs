//! A supported benchmark becoming an exception must fail the checksum gate.

#![cfg(unix)]
#![allow(clippy::expect_used, reason = "test process must start")]

#[test]
fn benchmark_exception_fails_the_quick_gate() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["bench", "--quick", "--engines", "micro-monty"])
        .env("MM", "/usr/bin/false")
        .output()
        .expect("xtask starts");
    assert!(
        !output.status.success(),
        "a Python exception is a regression"
    );
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("startup"), "{error}");
    assert!(error.contains("exit status: 1"), "{error}");
}

#[test]
fn comparator_failure_is_not_an_unsupported_feature() {
    for binary in ["/usr/bin/false", "/bin/echo"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["bench", "--quick", "--engines", "qjs"])
            .env("QJS", binary)
            .output()
            .expect("xtask starts");
        assert!(
            !output.status.success(),
            "{binary}: a comparator crash or wrong result must fail the gate"
        );
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("startup"), "{error}");
        assert!(!error.contains("unsupported"), "{error}");
    }
}

#[test]
fn explicitly_requested_missing_comparator_fails() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["bench", "--quick", "--engines", "qjs"])
        .env("QJS", "/nonexistent-micro-monty-benchmark-engine")
        .output()
        .expect("xtask starts");
    assert!(!output.status.success(), "a requested engine is required");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("qjs is required"), "{error}");
}

#[test]
fn benchmark_watchdog_reports_a_timeout_as_failure() {
    use std::{fs, os::unix::fs::PermissionsExt, time::Instant};

    let directory = std::env::temp_dir().join(format!("mm-bench-timeout-{}", std::process::id()));
    fs::create_dir_all(&directory).expect("temporary directory");
    let binary = directory.join("fixture");
    fs::write(
        &binary,
        "#!/bin/sh\nif [ \"$1\" = -h ]; then echo fixture; exit 0; fi\nexec sleep 60\n",
    )
    .expect("fixture script");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).expect("executable fixture");
    let start = Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([
            "bench",
            "--quick",
            "--engines",
            "qjs",
            "--timeout-secs",
            "1",
        ])
        .env("QJS", binary)
        .output()
        .expect("xtask starts");
    assert!(!output.status.success(), "a timeout must fail the gate");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("startup"), "{error}");
    assert!(error.contains("exit status: 124"), "{error}");
    assert!(
        start.elapsed().as_secs() < 10,
        "watchdog did not stop the run"
    );
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn a_timed_failure_after_passing_rounds_fails_and_removes_the_cell() {
    use std::{fs, os::unix::fs::PermissionsExt};

    let directory = std::env::temp_dir().join(format!("mm-bench-flaky-{}", std::process::id()));
    fs::create_dir_all(&directory).expect("temporary directory");
    let calls = directory.join("calls");
    let expected = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../bench/expected.txt"
    ))
    .expect("checksum oracle");
    let want = |program: &str| {
        expected
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{program}: ")))
            .expect("program checksum")
            .to_owned()
    };
    // fib passes its warm-up and first timed round, then prints a wrong result.
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = -h ]; then echo fixture; exit 0; fi\ncase \"$1\" in\n\
         */fib.js) echo x >> {calls}; [ $(wc -l < {calls}) -eq 3 ] && echo wrong || echo '{}';;\n\
         */startup.js) echo '{}';;\n*) exit 3;;\nesac\n",
        want("fib"),
        want("startup"),
        calls = calls.display(),
    );
    let binary = directory.join("fixture");
    fs::write(&binary, script).expect("fixture script");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).expect("executable fixture");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["bench", "--engines", "qjs", "--reps", "3", "--out-dir"])
        .arg(&directory)
        .env("QJS", &binary)
        .output()
        .expect("xtask starts");
    assert!(
        !output.status.success(),
        "a failed timed round must fail the run"
    );
    let summary = fs::read_to_string(directory.join("summary.md")).expect("summary");
    let row = |program: &str| {
        summary
            .lines()
            .find(|line| line.starts_with(&format!("| {program} |")))
            .expect("summary row")
            .to_owned()
    };
    assert!(row("fib").contains(" failed |"), "{summary}");
    assert!(!row("startup").contains("failed"), "{summary}");
    assert!(
        summary.contains("not inferred to be unsupported"),
        "{summary}"
    );
    let raw = fs::read_to_string(directory.join("samples.jsonl")).expect("raw samples");
    let fib: Vec<serde_json::Value> = raw
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON sample"))
        .filter(|s: &serde_json::Value| s["program"] == "fib")
        .collect();
    assert_eq!(fib.len(), 3, "rounds stop after the failure: {fib:?}");
    assert!(fib[2]["error"].is_string() && fib[1]["error"].is_null());
    fs::remove_dir_all(directory).expect("remove fixture");
}

#[test]
fn timed_reports_preserve_samples_and_exclude_historical_engines() {
    use std::{fmt::Write as _, fs, os::unix::fs::PermissionsExt};

    let directory = std::env::temp_dir().join(format!("mm-bench-reports-{}", std::process::id()));
    fs::create_dir_all(&directory).expect("temporary directory");
    fs::write(
        directory.join("cpython.txt"),
        "# engine: STALE ENGINE\nstartup 0 0\n",
    )
    .expect("historical result");
    let expected = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../bench/expected.txt"
    ))
    .expect("checksum oracle");
    let mut script = String::from(
        "#!/bin/sh\nif [ \"$1\" = -h ]; then echo fixture; exit 0; fi\ncase \"$1\" in\n",
    );
    for (name, want) in expected
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once(": "))
    {
        let stem = if name == "json" { "json_rows" } else { name };
        writeln!(script, "*/{stem}.js) printf '%s\\n' '{want}';;").expect("fixture source");
    }
    script.push_str("*) exit 2;;\nesac\n");
    let binary = directory.join("fixture");
    fs::write(&binary, script).expect("fixture script");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).expect("executable fixture");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["bench", "--engines", "qjs", "--reps", "2", "--out-dir"])
        .arg(&directory)
        .env("QJS", &binary)
        .output()
        .expect("xtask starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary = fs::read_to_string(directory.join("summary.md")).expect("summary");
    assert!(summary.contains("fixture"), "{summary}");
    assert!(!summary.contains("STALE ENGINE"), "{summary}");
    let raw = fs::read_to_string(directory.join("samples.jsonl")).expect("raw samples");
    let samples: Vec<serde_json::Value> = raw
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON sample"))
        .collect();
    assert_eq!(
        samples.len(),
        39,
        "13 programs, one warmup and two measured runs"
    );
    assert_eq!(samples.iter().filter(|s| s["warmup"] == false).count(), 26);
    assert!(
        samples
            .iter()
            .all(|s| s["error"].is_null() && s["exit_code"] == 0)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json")).expect("manifest"))
            .expect("JSON manifest");
    assert_eq!(
        manifest["engines"].as_array().expect("engine list").len(),
        1
    );
    assert!(manifest["engines"][0]["sha256"].is_string());
    fs::remove_dir_all(directory).expect("remove fixture");
}
