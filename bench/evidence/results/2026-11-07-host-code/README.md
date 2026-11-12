# Host-module code compiled once per Engine (2026-11-07)

HANDOFF "Scope decisions" 60 (pre-0.1 review M8). Before is `hd` built
from `357e003`; after is this change on `d2912a2`. The dunder-flag
commit between them moves these numbers by under 1% (the prelude's
classes); the rest is this change. x86-64 16-vCPU orb, release, timings pinned with
`taskset -c 2`. All numbers are [M].

## Commands

```bash
cargo build --release --manifest-path bench/evidence/host_defs/Cargo.toml \
    --target-dir target/host-defs
python3.14 -I bench/evidence/host_defs/measure.py OUT.json [--hd PATH]
python3 bench/callgrind.py --engine base="BASE/mm run" --engine new="target/release/mm run"
```

Each tool is a typed function with three annotated parameters (one
keyword-only), defaults and a docstring (`bench/evidence/host_defs/main.rs`).
`cold_process_ns` was added to `base.json` with the same `hd time … 1
cold` runs as `measure.py` makes; `hd-base` ignores `snap … stored` and
always stores its code, so its "code stored" rows are its only image.

- **New process and Engine:** the first define in a process, the cost a
  host pays once per Engine. Unchanged.
- **New Engine, later in the process:** the median of the next 20, each on
  a new Engine, with the earlier Engines alive. After the change those keep
  their compiled code, so each define runs in new memory; Ir is unchanged.
- **Warm:** an Engine that defined the modules before: the code is reused.
- **Ir:** steady per session, (Ir(11) − Ir(1)) / 10.
- **Image:** a session with the tools; zstd `--patch-from` a new
  session's image (41,081 B) as the startup prefix.
- **Restore, new Engine:** a keyed image on an Engine without the code
  regenerates and compiles it. **Code stored**: the same session on an
  Engine made with `with_host_code_in_snapshots(true)` (the mm-wasm
  guest's setting), whose images restore by decoding.

10/1 (tools/modules)
| | before | after |
|---|---:|---:|
| Session::new + define, new process and Engine (µs) | 873.02 | 874.84 |
| … new Engine, later in the process (µs) | 615.68 | 1,233.76 |
| … warm (µs) | 629.43 | 129.75 |
| Ir cold (M) | 6.93 | 6.88 |
| Ir warm (M) | 6.93 | 1.96 |
| RSS per warm session (KiB) | 259 | 136 |
| image (B) | 102,832 | 73,030 |
| zstd -1 (B) | 22,772 | 13,530 |
| zstd -1, startup prefix (B) | 15,295 | 6,315 |
| restore, new Engine (µs) | 125.15 | 599.67 |
| restore, warm Engine (µs) | 124.49 | 48.63 |
| image, code stored (B) | 102,832 | 94,339 |
| … zstd -1, prefix (B) | 15,295 | 14,317 |
| … restore, new Engine (µs) | 125.15 | 127.17 |

100/1 (tools/modules)
| | before | after |
|---|---:|---:|
| Session::new + define, new process and Engine (µs) | 6,492.45 | 6,460.03 |
| … new Engine, later in the process (µs) | 5,212.13 | 7,055.97 |
| … warm (µs) | 5,130.02 | 956.58 |
| Ir cold (M) | 61.58 | 61.69 |
| Ir warm (M) | 61.51 | 14.38 |
| RSS per warm session (KiB) | 1,796 | 596 |
| image (B) | 597,818 | 293,606 |
| zstd -1 (B) | 111,753 | 34,222 |
| zstd -1, startup prefix (B) | 106,861 | 27,948 |
| restore, new Engine (µs) | 850.19 | 6,624.19 |
| restore, warm Engine (µs) | 851.11 | 248.57 |
| image, code stored (B) | 597,818 | 517,350 |
| … zstd -1, prefix (B) | 106,861 | 103,620 |
| … restore, new Engine (µs) | 850.19 | 873.92 |

600/1 (tools/modules)
| | before | after |
|---|---:|---:|
| Session::new + define, new process and Engine (µs) | 41,237.56 | 42,831.04 |
| … new Engine, later in the process (µs) | 33,801.00 | 53,714.14 |
| … warm (µs) | 36,729.23 | 7,451.34 |
| Ir cold (M) | 386.96 | 389.32 |
| Ir warm (M) | 387.24 | 97.91 |
| RSS per warm session (KiB) | 10,242 | 2,432 |
| image (B) | 3,443,424 | 1,602,407 |
| zstd -1 (B) | 582,144 | 138,570 |
| zstd -1, startup prefix (B) | 593,342 | 136,319 |
| restore, new Engine (µs) | 5,300.43 | 45,628.42 |
| restore, warm Engine (µs) | 5,052.32 | 1,905.36 |
| image, code stored (B) | 3,443,424 | 2,956,406 |
| … zstd -1, prefix (B) | 593,342 | 577,305 |
| … restore, new Engine (µs) | 5,300.43 | 5,074.17 |

1200/12 (tools/modules)
| | before | after |
|---|---:|---:|
| Session::new + define, new process and Engine (µs) | 77,428.63 | 78,021.65 |
| … new Engine, later in the process (µs) | 69,625.62 | 129,829.90 |
| … warm (µs) | 75,244.17 | 27,512.59 |
| Ir cold (M) | 808.97 | 805.25 |
| Ir warm (M) | 809.11 | 239.21 |
| RSS per warm session (KiB) | 17,899 | 6,423 |
| image (B) | 6,304,453 | 2,645,657 |
| zstd -1 (B) | 634,463 | 226,971 |
| zstd -1, startup prefix (B) | 377,112 | 230,443 |
| restore, new Engine (µs) | 12,326.60 | 126,549.77 |
| restore, warm Engine (µs) | 11,645.21 | 5,170.69 |
| image, code stored (B) | 6,304,453 | 5,334,405 |
| … zstd -1, prefix (B) | 377,112 | 357,700 |
| … restore, new Engine (µs) | 12,326.60 | 10,569.41 |

## Files

- `base.json`, `new.json`: `measure.py`'s output before and after.
- `bench-ir.md`: whole-process callgrind Ir of the bench programs.
