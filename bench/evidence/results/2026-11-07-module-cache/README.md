# The Engine's decoded-module cache (2026-11-07)

HANDOFF "Scope decisions" 45. This change is on top of origin/main `7541558`.
x86-64 orb, release. All numbers are [M].

## Commands

```bash
cargo build --release --manifest-path bench/evidence/session_templates/Cargo.toml \
    --target-dir target/session-templates
python3.14 -I bench/evidence/session_templates/measure.py OUT/off --module-cache 0 --rss-n 1,8,40
python3.14 -I bench/evidence/session_templates/measure.py OUT/on --rss-n 1,8,40
python3 bench/callgrind.py --engine base="/tmp/mm-base run" --engine cache="target/release/mm run"
```

- `--module-cache 0` means `Engine::with_module_cache(config, 0)`: no cache,
  as before this change. The default is `Engine::new`, with an 8 MiB cap.
- **Ir:** `first` is the first operation in a process, with the cache cold.
  `steady` is the mean of the next ten, with the cache warm.
- **RSS:** the per-session growth while holding N sessions, after one
  warm-up operation.
- `/tmp/mm-base` is `mm` built from `7541558`.

## Files

- `templates-cache-{off,on}.{md,json}`: the session-templates table with
  and without the cache, including RSS at N = 1, 8 and 40 and the bytes the
  cache holds.
- `bench-ir.md`: whole-process callgrind Ir of the 13 bench programs, base
  against cache.

## Results

| Set | import Ir, cold → warm | restore Ir, cold → warm | uncached steady import / restore | cache holds |
|---|---:|---:|---:|---:|
| codemode | 42.39M → 28.60M | 26.09M → 9.95M | 42.60M / 23.75M | 607,294 B |
| codemode + tz | 85.67M → 61.42M | 41.96M → 14.13M | 86.87M / 39.53M | 1,115,868 B |
| heavy | 212.32M → 169.95M | 62.21M → 16.56M | 213.52M / 60.22M | 1,874,424 B |

- `empty` and `json` load no cached Python module, so they are unchanged.
- Decoding every bundled module fills the cache with 2,913,194 B.

RSS per session, KiB, without → with the cache:

| Set | fresh N=8 | fresh N=40 | restore N=8 | restore N=40 |
|---|---:|---:|---:|---:|
| codemode | 1,736 → 1,176 | 1,856 → 1,204 | 1,713 → 1,086 | 1,847 → 1,181 |
| codemode + tz | 3,048 → 1,938 | 3,237 → 1,998 | 2,936 → 1,814 | 3,176 → 1,934 |
| heavy | 5,254 → 3,398 | 5,493 → 3,452 | 4,990 → 3,102 | 5,333 → 3,247 |

At N = 1, the growth is at page granularity and mostly 0 or one
allocation's worth (see the JSON).

**Bench Ir.**

- 12 of the 13 programs are within ±0.01%.
- `startup` (`print(0)`) is 885,180 → 893,385 Ir (+0.93%) with both
  binaries at paths of the same length.
  - About 6,200 Ir of that is libc parsing `/proc/self/maps` at process
    start (`__vfscanf_internal`, `strtoul`, `getline`). Under valgrind,
    the larger binary has two more mappings before `[stack]` (49 lines
    against 47).
  - The engine's part is about 1,450 Ir (+0.16%): `Engine::new` 704 Ir,
    plus the cache's drop and allocations.
  - A build of the same code with symbols counts 886,627 Ir.
  - The owner accepted this +0.93% as an exception to the ±0.5% bench
    rule (HANDOFF "Scope decisions" 45, 2026-11-07).

**Size** (`cargo xtask size`, against `7541558`):

- `hello.wasm` 2,101,121 → 2,102,983 B (+1,862);
- native `hello` 2,303,928 → 2,306,136 B (+2,208);
- mm-wasm guest 2,205,141 → 2,206,824 B (+1,683).

The ceilings were raised to the measurement plus about 2 KB.
