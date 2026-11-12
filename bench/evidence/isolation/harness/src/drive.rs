//! Workloads and the host side of a cell: tool answers, batches, fuel slices.

use std::collections::HashMap;

use iso_service::{Request, Response, ResumeMsg};
use serde_json::{Value as Json, json};

use crate::backend::Backend;

/// The host functions every session defines: `tool` answers in place,
/// `atool` answers with a future (batched at `Waiting`).
pub const TOOLS: [&str; 2] = ["tool", "atool"];

/// A codemode-style cell: 200 sync tool calls, then 10 `asyncio.gather`
/// batches of 20 async tool calls; tools return JSON text, as in
/// programmatic tool calling.
pub const CODEMODE: &str = r#"import json, asyncio
total = 0
for i in range(200):
    r = json.loads(tool({"i": i, "q": "item"}))
    total += r["i"]
async def main():
    acc = 0
    for b in range(10):
        rs = await asyncio.gather(*[atool({"i": b * 20 + j}) for j in range(20)])
        acc += sum(json.loads(r)["i"] for r in rs)
    return acc
print(total, asyncio.run(main()))
"#;
pub const CODEMODE_OUT: &str = "19900 19900\n";
pub const CODEMODE_CALLS: u64 = 400;

/// `n` sync host calls with an int argument echoed back.
pub fn call_loop(n: u64) -> String {
    format!("t = 0\nfor i in range({n}):\n    t += tool(i)\nprint(t)\n")
}

/// The same loop calling a Python function instead of the host.
pub fn py_loop(n: u64) -> String {
    format!("def f(x):\n    return x\nt = 0\nfor i in range({n}):\n    t += f(i)\nprint(t)\n")
}

/// The tool's answer: an int echoes, a dict gets a JSON string.
pub fn answer(args: &[Json]) -> Json {
    match args.first() {
        Some(Json::Number(n)) => Json::Number(n.clone()),
        Some(Json::Object(o)) => Json::String(
            json!({"i": o.get("i").cloned().unwrap_or(Json::Null), "ok": true, "data": "x".repeat(64)})
                .to_string(),
        ),
        _ => Json::Null,
    }
}

/// How a cell ended, with the host's counters.
#[derive(Debug)]
pub struct Outcome {
    pub response: Response,
    pub host_calls: u64,
    #[allow(dead_code, reason = "kept for debugging output")]
    pub fuel_parks: u64,
}

/// Options for [`drive`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Drive {
    /// Refill this much engine fuel at each fuel park (a sliced run).
    pub slice: Option<u64>,
    /// Stop (return) at the n-th host call instead of answering it.
    pub stop_at_call: Option<u64>,
}

/// Answers stops until the cell completes, errors, parks for another
/// reason, or reaches `stop_at_call`.
pub fn drive(b: &mut dyn Backend, first: Response, opts: Drive) -> Result<Outcome, String> {
    let mut response = first;
    let mut pending: HashMap<u64, Json> = HashMap::new();
    let (mut host_calls, mut fuel_parks) = (0, 0);
    loop {
        let (resume, fuel) = match &response {
            Response::HostCall { call, name, args } => {
                host_calls += 1;
                if opts.stop_at_call == Some(host_calls) {
                    break;
                }
                if name == "atool" {
                    pending.insert(*call, answer(args));
                    (ResumeMsg::Future, None)
                } else {
                    (
                        ResumeMsg::Value {
                            value: answer(args),
                        },
                        None,
                    )
                }
            }
            Response::Waiting { calls } => {
                let results = calls
                    .iter()
                    .map(|c| (*c, pending.remove(c).unwrap_or(Json::Null)))
                    .collect();
                (ResumeMsg::Batch { results }, None)
            }
            Response::Parked { reason } if reason == "fuel" && opts.slice.is_some() => {
                fuel_parks += 1;
                (ResumeMsg::Continue, opts.slice)
            }
            _ => break,
        };
        response = b.call(&Request::Resume { resume, fuel }, &[])?.0;
    }
    Ok(Outcome {
        response,
        host_calls,
        fuel_parks,
    })
}

/// A fresh session (`memory` bytes) with the tools defined.
pub fn new_session(b: &mut dyn Backend, memory: u64) -> Result<(), String> {
    match b
        .call(
            &Request::NewSession {
                memory,
                define: TOOLS.iter().map(|s| (*s).to_owned()).collect(),
            },
            &[],
        )?
        .0
    {
        Response::Ok => Ok(()),
        other => Err(format!("new session: {other:?}")),
    }
}

/// Runs `source` to its end with tools answered.
pub fn run(b: &mut dyn Backend, source: &str, opts: Drive) -> Result<Outcome, String> {
    let fuel = opts.slice;
    let first = b
        .call(
            &Request::Run {
                source: source.to_owned(),
                fuel,
            },
            &[],
        )?
        .0;
    drive(b, first, opts)
}

/// The cell's printed output, or a description of how it ended.
pub fn output(outcome: &Outcome) -> String {
    match &outcome.response {
        Response::Complete { output, .. } => output.clone(),
        other => format!("{other:?}"),
    }
}

/// The bench programs and their expected lines (`bench/expected.txt`).
pub fn bench_programs(root: &std::path::Path) -> Vec<(String, String, String)> {
    let expected = std::fs::read_to_string(root.join("bench/expected.txt")).unwrap_or_default();
    expected
        .lines()
        .filter(|l| !l.starts_with('#') && l.contains(": "))
        .filter_map(|l| {
            let (name, line) = l.split_once(": ")?;
            let file = if name == "json" { "json_rows" } else { name };
            let source = std::fs::read_to_string(root.join(format!("bench/{file}.py"))).ok()?;
            Some((file.to_owned(), source, format!("{line}\n")))
        })
        .collect()
}
