//! The size measurement's minimal djs host: one script (argv[1]) on a VM
//! from djs's base snapshot, `console.log` through a host function to
//! stdout.

use djs_core::compiler::CompileOptions;
use djs_core::value::Value;
use djs_core::vm::{Status, VmOptions};

fn main() {
    let src = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "console.log('hello')".to_owned());
    let Ok(mut vm) = djs_prelude::new_vm(VmOptions::default()) else {
        std::process::exit(1)
    };
    let Ok(log) = vm.new_host_function(1, "log") else {
        std::process::exit(1)
    };
    if vm.define_global("__log", log).is_err() {
        std::process::exit(1);
    }
    let console = "globalThis.console = { log: (...a) => __log(a.map(String).join(' ')) };";
    for (code, name) in [(console, "console.js"), (src.as_str(), "main.js")] {
        let Ok(f) = vm.compile(code.as_bytes(), &CompileOptions::script(name)) else {
            std::process::exit(1)
        };
        let this = Value::object(vm.global());
        if vm.call(f, this, &[]).is_err() {
            std::process::exit(1);
        }
        loop {
            match vm.run(u64::MAX) {
                Status::HostCall { call_id, .. } => {
                    let text = vm
                        .host_call(call_id)
                        .and_then(|c| c.args.first().copied())
                        .and_then(|v| v.as_string())
                        .map(|s| vm.heap().str_to_string(s))
                        .unwrap_or_default();
                    println!("{text}");
                    if vm.resolve_host_call(call_id, Ok(Value::UNDEFINED)).is_err() {
                        std::process::exit(1);
                    }
                }
                Status::Done(_) => break,
                _ => std::process::exit(1),
            }
        }
    }
}
