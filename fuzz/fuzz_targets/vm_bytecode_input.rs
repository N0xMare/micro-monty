//! The `vm_bytecode` fuzz target's input decoding and driver, shared with
//! the regression test that replays its seeds
//! (`crates/micro-monty/tests/session.rs`), so a seed means the same edits
//! to the same program in both.

use mm_compile::bytecode::Program;
use mm_engine::{Exit, Vm, VmConfig};
use std::sync::OnceLock;

const PROGRAMS: [&str; 6] = [
    "x = [i * i for i in range(20) if i % 3]\nd = {str(i): i for i in x}\nprint(sum(d.values()), sorted(d)[:3])\n",
    "def f(n):\n    if n < 2:\n        return n\n    return f(n - 1) + f(n - 2)\nclass C:\n    def __init__(self, v):\n        self.v = v\n    def __repr__(self):\n        return f'C({self.v!r})'\n    def __add__(self, o):\n        return C(self.v + o.v)\nprint(C(f(10)) + C(2), [C(i) for i in range(3)])\n",
    "def g(n):\n    try:\n        for i in range(n):\n            yield i\n    finally:\n        print('done')\nt = 0\nfor v in g(10):\n    t += v\ntry:\n    1 / 0\nexcept ZeroDivisionError as e:\n    print(type(e).__name__, e, t)\n",
    "def outer():\n    n = 0\n    def inc(k=1):\n        nonlocal n\n        n += k\n        return n\n    return inc\ni = outer()\nprint(i(), i(5), (lambda *a, **k: (a, k))(1, 2, z=3))\nclass M:\n    def __enter__(self):\n        return 3\n    def __exit__(self, *a):\n        return True\nwith M() as c:\n    print(c)\n    raise KeyError(c)\n",
    "match [1, (2, 3), {'k': 'v'}]:\n    case [int(a), (b, *rest), {'k': str() as s}] if a < b:\n        print(a, b, rest, s)\n    case _:\n        print('no')\ns = 'hello world'\nprint(s[::-2], s.split(), f'{3.14159:>10.3f}|{2**70:,}|{s!r:^20}')\n",
    "import asyncio\nasync def one(i):\n    await asyncio.sleep(0)\n    return i * 2\nasync def main():\n    return await asyncio.gather(*[one(i) for i in range(4)])\nprint(asyncio.run(main()), {1, 2} | {3}, (1, 2) < (1, 3), 7 // 2, -7 % 3, 2 ** -1)\n",
];

fn bases() -> &'static [Vec<u8>] {
    static BASES: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    BASES.get_or_init(|| {
        PROGRAMS
            .iter()
            .map(|src| {
                let module = mm_parse::parse(src).expect("parses");
                let opts = mm_compile::Options {
                    last_expr_value: true,
                };
                mm_compile::compile(&module, opts)
                    .expect("compiles")
                    .encode()
            })
            .collect()
    })
}

/// One fuzz input: edits to an encoded program (see the target's doc
/// comment); what decodes and verifies runs with a fuel budget.
pub fn run(data: &[u8]) {
    let Some((&pick, edits)) = data.split_first() else {
        return;
    };
    let mut bytes = bases()[pick as usize % PROGRAMS.len()].clone();
    for ed in edits.as_chunks::<4>().0 {
        if bytes.is_empty() {
            break;
        }
        let at = usize::from(u16::from_le_bytes([ed[0], ed[1]])) % bytes.len();
        if ed[3] == 0xFF {
            bytes.truncate(at);
        } else {
            bytes[at] ^= ed[2];
        }
    }
    let Some(program) = Program::decode(&bytes) else {
        return;
    };
    if program.codes.is_empty() || !program.codes.iter().all(|c| c.verify()) {
        return;
    }
    let config = VmConfig {
        heap: mm_core::HeapConfig {
            memory_limit: 64 << 20,
            ..mm_core::HeapConfig::default()
        },
        recursion_limit: 200,
        fuel: Some(20_000),
        stdlib: mm_stdlib::MODULES,
    };
    let Ok(mut vm) = Vm::new(config) else {
        return;
    };
    vm.set_output(Box::new(|_, _: &str| {}));
    let Ok(entry) = vm.load(&program, "fuzz.py", "") else {
        return;
    };
    let mut exit = vm.run(entry);
    // Host calls and waits cannot be answered here; parks end the run.
    // Capped as the facade renders a completion (1 MiB, its default text
    // limit), not the whole text up to the memory limit.
    if let Exit::Complete(v) = exit {
        let _ = vm.repr_capped_of(v, 1 << 20);
    }
    for _ in 0..2 {
        match exit {
            Exit::Parked(_) => {
                vm.set_fuel(Some(5_000));
                exit = vm.resume();
            }
            _ => break,
        }
    }
    let mut out = Vec::new();
    let _ = vm.write_snapshot(&mut out);
}
