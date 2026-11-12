//! `cargo xtask difffuzz`: the differential fuzzer against CPython 3.14
//! (docs/spec.md §13). Builds `mm` and runs `fuzz/difffuzz/difffuzz.py`,
//! which generates one subset program per seed, runs it on both engines and
//! saves each new divergence, minimized, under `fuzz/difffuzz/divergences/`
//! (at most two per signature). Divergences are findings, not failures:
//! the task fails only if the harness does.

use std::process::Command;

use crate::{cpython_oracle_version, micro_monty_binary, python, workspace_root};

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut pass = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--seeds" | "--start" | "--jobs" | "--minutes" | "--timeout" | "--out" => {
                let value = it.next().ok_or(format!("{arg} needs a value"))?;
                pass.extend([arg.clone(), value.clone()]);
            }
            other => return Err(format!("difffuzz: unknown option `{other}`")),
        }
    }
    let python = python();
    let version = cpython_oracle_version(&python)?;
    let mm = micro_monty_binary()?;
    println!(
        "difffuzz: micro-monty ({}) against CPython {version}",
        mm.display()
    );
    let status = Command::new(&python)
        .arg(workspace_root().join("fuzz/difffuzz/difffuzz.py"))
        .arg("--mm")
        .arg(&mm)
        .arg("--python")
        .arg(&python)
        .args(&pass)
        .status()
        .map_err(|error| format!("cannot run the differential fuzzer: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("the differential fuzzer failed".to_owned())
    }
}
