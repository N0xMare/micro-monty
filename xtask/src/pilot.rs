//! The four-orb release-gate pilot. The plan is derived from the local
//! gate graph at the same committed head on every worker. No build or
//! measurement produced on one worker is consumed on another.

use crate::{check, check::Stage, wasm_wall::git};
use serde_json::{Value, json};
use std::{env, fs, fs::OpenOptions, io::Write as _, path::Path};

pub(crate) enum Pilot<'a> {
    Plan {
        id: &'a str,
        path: &'a str,
    },
    Run {
        plan: &'a str,
        result: &'a str,
        stages: &'a str,
    },
    Join {
        plan: &'a str,
        results: &'a [String],
    },
}

pub(crate) fn parse(args: &[String]) -> Result<(Option<Pilot<'_>>, &[String]), String> {
    let Some(at) = args.iter().position(|arg| arg.starts_with("--pilot-")) else {
        return Ok((None, args));
    };
    let (common, mode) = args.split_at(at);
    let pilot = match mode {
        [flag, id, path] if flag == "--pilot-plan" && !id.is_empty() => Pilot::Plan { id, path },
        [flag, plan, result, stages] if flag == "--pilot-run" => Pilot::Run { plan, result, stages },
        [flag, plan, results @ ..] if flag == "--pilot-join" => Pilot::Join { plan, results },
        _ => return Err("check: expected --pilot-plan ID PLAN, --pilot-run PLAN RESULT STAGE[,STAGE...], or --pilot-join PLAN RESULT...".into()),
    };
    Ok((Some(pilot), common))
}

impl Pilot<'_> {
    pub(crate) fn execute(
        self,
        root: &Path,
        stages: Vec<Stage>,
        head: &str,
        base: &str,
        ir_base: &str,
        host: &str,
    ) -> Result<(), String> {
        require_checkout(root, head)?;
        for name in [
            "MM_IR_ACCEPT",
            "MM_WASM_IR_ACCEPT",
            "MM_V8_IR_ACCEPT",
            "MM_BLESS",
        ] {
            if env::var_os(name).is_some() {
                return Err(format!("check: the pilot requires {name} to be unset"));
            }
        }
        let lanes = check::pilot_lanes(&stages)?;
        let make_plan = |id: &str| {
            json!({
                "version": 1, "id": id, "head": head, "base": base, "ir_base": ir_base,
                "host_target": host, "lanes": lanes,
            })
        };
        let read_plan = |path: &str| {
            let plan = read(path)?;
            if plan["id"]
                .as_str()
                .is_some_and(|id| !id.is_empty() && plan == make_plan(id))
            {
                Ok(plan)
            } else {
                Err(format!(
                    "check: plan {path} does not match this checkout, bases, target or stage graph"
                ))
            }
        };
        match self {
            Self::Plan { id, path } => write_new(path, &make_plan(id)),
            Self::Run {
                plan,
                result,
                stages: selected,
            } => {
                let plan = read_plan(plan)?;
                let selected: Vec<_> = selected.split(',').collect();
                let lane = lanes
                    .iter()
                    .position(|lane| *lane == selected)
                    .ok_or("check: give exactly one planned stage list, in plan order")?;
                if Path::new(result).exists() {
                    return Err(format!("check: result path already exists: {result}"));
                }
                let mut report = Value::Null;
                let outcome = check::run(
                    check::keep(stages, &selected)?,
                    root,
                    false,
                    Some(&mut report),
                );
                require_checkout(root, head)?;
                if !report.is_null() {
                    write_new(
                        result,
                        &json!({"plan": plan, "lane": lane, "report": report}),
                    )?;
                }
                outcome
            }
            Self::Join { plan, results } => {
                let plan = read_plan(plan)?;
                join(&plan, &lanes, results.iter().map(|path| read(path)))?;
                println!("check: all pilot lanes passed for {head}");
                Ok(())
            }
        }
    }
}

fn require_checkout(root: &Path, head: &str) -> Result<(), String> {
    if git(root, &["rev-parse", "HEAD"])? != head
        || !git(root, &["status", "--porcelain"])?.is_empty()
    {
        return Err("check: the pilot requires an unchanged, clean checkout".into());
    }
    Ok(())
}

fn read(path: &str) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|e| format!("check: cannot read {path}: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("check: invalid JSON in {path}: {e}"))
}

fn write_new(path: &str, value: &Value) -> Result<(), String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{value:#}"))
        .map_err(|e| format!("check: cannot write new file {path}: {e}"))
}

fn join(
    plan: &Value,
    lanes: &[Vec<&str>],
    results: impl IntoIterator<Item = Result<Value, String>>,
) -> Result<(), String> {
    let mut seen = vec![false; lanes.len()];
    let mut failures = Vec::new();
    for result in results {
        let result = result?;
        if result["plan"] != *plan {
            return Err("check: result belongs to a different pilot plan".into());
        }
        let lane = result["lane"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n < lanes.len())
            .ok_or("check: result has no planned lane")?;
        if std::mem::replace(&mut seen[lane], true) {
            return Err(format!("check: duplicate result for pilot lane {lane}"));
        }
        let report = &result["report"];
        let elapsed = report["elapsed_ms"]
            .as_u64()
            .ok_or("check: missing lane duration")?;
        let rows = report["rows"]
            .as_array()
            .filter(|rows| rows.len() == lanes[lane].len())
            .ok_or_else(|| format!("check: incomplete stage results for lane {lane}"))?;
        for (row, name) in rows.iter().zip(&lanes[lane]) {
            if row["stage"] != *name || row["ms"].as_u64().is_none_or(|ms| ms > elapsed) {
                return Err(format!(
                    "check: mismatched stage result for {name} in lane {lane}"
                ));
            }
            match row["status"].as_str() {
                Some("passed") => (),
                Some(status @ ("failed" | "stopped" | "not run")) => {
                    failures.push(format!("{name}: {status}"));
                }
                _ => return Err(format!("check: invalid stage status for {name}")),
            }
        }
    }
    for (lane, present) in seen.into_iter().enumerate() {
        if !present {
            failures.push(format!("missing lane {lane} ({})", lanes[lane].join(", ")));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check: pilot did not pass: {}",
            failures.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_requires_exactly_one_passing_result_per_planned_lane() {
        let lanes = [vec!["build", "fuel"], vec!["timed"]];
        let plan = json!({"id": "trial-a", "lanes": lanes});
        let receipt = |lane: usize, status: &str| {
            let rows: Vec<_> = lanes[lane]
                .iter()
                .map(|name| json!({"stage": name, "status": status, "ms": 74}))
                .collect();
            json!({"plan": plan, "lane": lane, "report": {"elapsed_ms": 130, "rows": rows}})
        };
        let untimed = receipt(0, "passed");
        let timed = receipt(1, "passed");
        let trial = |results: &[Value]| join(&plan, &lanes, results.iter().cloned().map(Ok));
        assert!(trial(&[timed.clone(), untimed.clone()]).is_ok());
        assert!(
            matches!(trial(std::slice::from_ref(&untimed)), Err(e) if e.contains("missing lane 1"))
        );
        assert!(
            matches!(trial(&[untimed.clone(), untimed.clone()]), Err(e) if e.contains("duplicate"))
        );
        assert!(
            matches!(trial(&[untimed.clone(), receipt(1, "failed")]), Err(e) if e.contains("timed: failed"))
        );
        let mut other = timed.clone();
        other["plan"]["id"] = json!("trial-b");
        assert!(
            matches!(trial(&[untimed.clone(), other]), Err(e) if e.contains("different pilot plan"))
        );
        let mut wrong_stage = timed;
        wrong_stage["report"]["rows"][0]["stage"] = json!("fuel");
        assert!(matches!(trial(&[untimed, wrong_stage]), Err(e) if e.contains("mismatched stage")));
    }
}
