## Wall time, ms: median / min [M]

| kernel | stack/match | stack/fntable | accumulator/match | accumulator/fntable | register3/match | register3/fntable | cpython | qjs |
|---|---|---|---|---|---|---|---|---|
| fib | 18.6 / 18.5 | 27.9 / 27.7 | 17.9 / 17.7 | 29.4 / 29.2 | 18.1 / 17.8 | 25.9 / 25.5 | 28.1 / 25.4 | 19.1 / 18.8 |
| loop | 39.5 / 39.0 | 62.5 / 62.0 | 31.1 / 30.8 | 49.2 / 48.5 | 31.4 / 31.2 | 41.5 / 41.1 | 100.1 / 97.5 | 63.9 / 63.4 |
| closure | 31.0 / 30.7 | 47.0 / 46.5 | 25.5 / 25.3 | 37.8 / 37.1 | 33.7 / 33.5 | 45.8 / 45.6 | 55.3 / 54.0 | 33.7 / 33.4 |
| attr | 32.1 / 31.7 | 47.5 / 47.1 | 27.1 / 26.9 | 42.8 / 42.3 | 29.6 / 29.5 | 38.4 / 38.2 | 56.0 / 55.3 | 35.2 / 35.1 |
| method | 34.9 / 34.6 | 52.9 / 52.5 | 32.0 / 31.9 | 46.5 / 46.1 | 36.4 / 36.1 | 50.8 / 50.4 | 55.1 / 54.6 | 63.4 / 63.2 |
| list | 45.9 / 45.2 | 68.8 / 68.4 | 40.2 / 39.7 | 59.1 / 58.6 | 42.9 / 42.5 | 56.8 / 56.4 | 78.1 / 76.2 | 72.8 / 72.4 |
| dict | 24.3 / 19.8 | 32.4 / 23.6 | 17.8 / 15.0 | 24.2 / 19.0 | 19.9 / 15.5 | 23.8 / 17.7 | 23.8 / 21.0 | 56.5 / 46.8 |

## Median relative to stack/match (lower is faster) [M]

| kernel | stack/match | stack/fntable | accumulator/match | accumulator/fntable | register3/match | register3/fntable | cpython | qjs |
|---|---|---|---|---|---|---|---|---|
| fib | 1.00 | 1.50 | 0.96 | 1.58 | 0.97 | 1.39 | 1.51 | 1.03 |
| loop | 1.00 | 1.58 | 0.79 | 1.25 | 0.79 | 1.05 | 2.53 | 1.62 |
| closure | 1.00 | 1.52 | 0.82 | 1.22 | 1.09 | 1.48 | 1.78 | 1.09 |
| attr | 1.00 | 1.48 | 0.84 | 1.33 | 0.92 | 1.20 | 1.74 | 1.10 |
| method | 1.00 | 1.51 | 0.92 | 1.33 | 1.04 | 1.45 | 1.58 | 1.82 |
| list | 1.00 | 1.50 | 0.87 | 1.29 | 0.93 | 1.24 | 1.70 | 1.59 |
| dict | 1.00 | 1.33 | 0.73 | 0.99 | 0.82 | 0.98 | 0.98 | 2.32 |

## Instructions dispatched per run, bytecode bytes, static instructions [M]

| kernel | stack dispatched | accumulator dispatched | register3 dispatched | stack bytes | accumulator bytes | register3 bytes | stack static | accumulator static | register3 static |
|---|---|---|---|---|---|---|---|---|---|
| fib | 6,356,209 | 7,309,641 | 4,767,157 | 37 | 53 | 53 | 21 | 26 | 17 |
| loop | 18,000,008 | 15,000,011 | 9,000,006 | 27 | 33 | 26 | 13 | 15 | 8 |
| closure | 10,000,015 | 8,000,020 | 9,000,012 | 48 | 58 | 60 | 24 | 27 | 20 |
| attr | 12,000,008 | 11,000,011 | 7,000,006 | 39 | 48 | 42 | 19 | 21 | 12 |
| method | 12,000,008 | 10,000,011 | 9,000,006 | 38 | 46 | 45 | 19 | 20 | 14 |
| list | 16,000,014 | 12,000,020 | 9,000,011 | 56 | 69 | 59 | 28 | 30 | 18 |
| dict | 2,800,014 | 2,000,020 | 1,400,011 | 53 | 63 | 53 | 26 | 28 | 16 |

Dispatched relative to stack:

| kernel | accumulator | register3 |
|---|---|---|
| fib | 1.15 | 0.75 |
| loop | 0.83 | 0.50 |
| closure | 0.80 | 0.90 |
| attr | 0.92 | 0.58 |
| method | 0.83 | 0.75 |
| list | 0.75 | 0.56 |
| dict | 0.71 | 0.50 |

## Median ns per dispatched instruction [M]

| kernel | stack/match | stack/fntable | accumulator/match | accumulator/fntable | register3/match | register3/fntable |
|---|---|---|---|---|---|---|
| fib | 2.92 | 4.38 | 2.44 | 4.02 | 3.79 | 5.42 |
| loop | 2.19 | 3.47 | 2.07 | 3.28 | 3.49 | 4.61 |
| closure | 3.10 | 4.70 | 3.18 | 4.72 | 3.75 | 5.09 |
| attr | 2.68 | 3.96 | 2.46 | 3.89 | 4.23 | 5.49 |
| method | 2.91 | 4.41 | 3.20 | 4.65 | 4.04 | 5.64 |
| list | 2.87 | 4.30 | 3.35 | 4.92 | 4.77 | 6.31 |
| dict | 8.69 | 11.56 | 8.89 | 12.10 | 14.22 | 16.99 |

## Handler code size, bytes [M]

| variant | bytes | functions | detail |
|---|---|---|---|
| stack/match | 6,123 | 1 | run_match |
| stack/fntable | 6,823 | 26 | run_fntable 128 + 25 handler fns (incl. merged/shared: acc::jump_loop vm::bad_op) |
| accumulator/match | 6,291 | 1 | run_match |
| accumulator/fntable | 7,632 | 26 | run_fntable 128 + 25 handler fns (incl. merged/shared: vm::bad_op) |
| register3/match | 6,532 | 1 | run_match |
| register3/fntable | 7,706 | 23 | run_fntable 128 + 22 handler fns (incl. merged/shared: acc::jump_loop acc::make_cell vm::bad_op) |
| shared-runtime | 7,640 | 20 | slow paths, dict/list helpers, natives (used by all six) |

## Bounds checks: checked vs `--features unchecked`, median ms [M]

| kernel | stack/match | stack/fntable | accumulator/match | accumulator/fntable | register3/match | register3/fntable |
|---|---|---|---|---|---|---|
| fib | 18.6 → 17.4 (-6%) | 27.9 → 23.7 (-15%) | 17.9 → 16.2 (-9%) | 29.4 → 26.2 (-11%) | 18.1 → 14.8 (-18%) | 25.9 → 22.3 (-14%) |
| loop | 39.5 → 30.5 (-23%) | 62.5 → 46.9 (-25%) | 31.1 → 22.8 (-27%) | 49.2 → 41.4 (-16%) | 31.4 → 22.7 (-28%) | 41.5 → 32.3 (-22%) |
| closure | 31.0 → 27.2 (-12%) | 47.0 → 38.0 (-19%) | 25.5 → 19.9 (-22%) | 37.8 → 30.6 (-19%) | 33.7 → 24.6 (-27%) | 45.8 → 36.3 (-21%) |
| attr | 32.1 → 25.2 (-22%) | 47.5 → 38.2 (-20%) | 27.1 → 19.7 (-27%) | 42.8 → 35.0 (-18%) | 29.6 → 20.9 (-29%) | 38.4 → 30.0 (-22%) |
| method | 34.9 → 30.1 (-14%) | 52.9 → 43.8 (-17%) | 32.0 → 25.9 (-19%) | 46.5 → 39.2 (-16%) | 36.4 → 26.2 (-28%) | 50.8 → 41.0 (-19%) |
| list | 45.9 → 37.3 (-19%) | 68.8 → 57.8 (-16%) | 40.2 → 30.5 (-24%) | 59.1 → 48.2 (-18%) | 42.9 → 32.2 (-25%) | 56.8 → 44.7 (-21%) |
| dict | 24.3 → 19.4 (-20%) | 32.4 → 25.4 (-22%) | 17.8 → 18.1 (+2%) | 24.2 → 20.0 (-17%) | 19.9 → 17.9 (-10%) | 23.8 → 21.1 (-11%) |

## Run-to-run: second process, same binary, median change vs the first [M]

| kernel | stack/match | stack/fntable | accumulator/match | accumulator/fntable | register3/match | register3/fntable |
|---|---|---|---|---|---|---|
| fib | +4% | +5% | +4% | +2% | +3% | +2% |
| loop | +4% | +4% | +3% | +1% | +3% | +3% |
| closure | +3% | +1% | -6% | +0% | +4% | +18% |
| attr | -0% | +3% | +4% | +7% | +3% | +2% |
| method | +1% | +3% | +1% | +3% | +1% | +2% |
| list | +3% | +4% | +3% | +3% | +3% | +3% |
| dict | +22% | +9% | +20% | +28% | +11% | +34% |
