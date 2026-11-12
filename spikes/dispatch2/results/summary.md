## Instructions, whole process (callgrind) [M]

| program | dispatch2 | qjs | mm | dispatch2 / qjs | mm / qjs | dispatch2 / mm |
|---|---|---|---|---|---|---|
| fib | 235.2M | 252.3M | 402.5M | 0.93 | 1.59 | 0.58 |
| closure | 514.3M | 539.6M | 975.0M | 0.95 | 1.81 | 0.53 |
| callfn | 369.3M | 504.6M | 681.0M | 0.73 | 1.35 | 0.54 |
| loopfn | 123.3M | 238.6M | 275.4M | 0.52 | 1.15 | 0.45 |
| loop | 867.3M | 838.4M | 1,476.4M | 1.03 | 1.76 | 0.59 |
| dictint | 347.5M | 368.7M | 532.7M | 0.94 | 1.44 | 0.65 |
| empty | 0.3M | 1.5M | 10.8M | 0.21 | 7.13 | 0.03 |

## Instructions net of each engine's empty program [M]

| program | dispatch2 | qjs | mm | dispatch2 / qjs | mm / qjs |
|---|---|---|---|---|---|
| fib | 234.9M | 250.8M | 391.7M | 0.94 | 1.56 |
| closure | 514.0M | 538.1M | 964.3M | 0.96 | 1.79 |
| callfn | 369.0M | 503.1M | 670.2M | 0.73 | 1.33 |
| loopfn | 123.0M | 237.1M | 264.7M | 0.52 | 1.12 |
| loop | 867.0M | 836.9M | 1,465.7M | 1.04 | 1.75 |
| dictint | 347.2M | 367.2M | 522.0M | 0.95 | 1.42 |

## Instructions per call [M]

| measure | dispatch2 | qjs | mm |
|---|---|---|---|
| call statement `total = inc(total)` minus `total = total + 1` (callfn − loopfn) / 1M | 246 | 266 | 406 |
| fib, all instructions per call (635,621 calls) | 370 | 395 | 616 |
| closure, all instructions per iteration (1 call each) | 514 | 538 | 964 |

## Wall time, ms, whole process: median of 21 pinned interleaved rounds [M]

| program | dispatch2 run1 | dispatch2 run2 | qjs run1 | qjs run2 | mm run1 | mm run2 | dispatch2 / qjs | mm / qjs |
|---|---|---|---|---|---|---|---|---|
| fib | 25.9 | 25.3 | 24.9 | 23.9 | 34.4 | 33.9 | 1.05 | 1.40 |
| closure | 40.0 | 39.1 | 43.9 | 41.9 | 72.9 | 68.0 | 0.92 | 1.64 |
| callfn | 28.6 | 28.6 | 41.1 | 39.8 | 49.6 | 47.2 | 0.71 | 1.20 |
| loopfn | 12.2 | 11.6 | 24.3 | 23.8 | 21.5 | 21.4 | 0.50 | 0.89 |
| loop | 54.6 | 54.4 | 71.9 | 70.1 | 95.8 | 94.6 | 0.77 | 1.34 |
| dictint | 38.9 | 37.7 | 96.6 | 92.3 | 53.1 | 51.1 | 0.41 | 0.55 |
| empty | 3.2 | 3.1 | 3.8 | 3.5 | 5.6 | 5.5 | 0.86 | 1.52 |

## Wall time net of each engine's empty program, ms (mean of the runs' medians) [M]

| program | dispatch2 | qjs | mm | dispatch2 / qjs | mm / qjs | projected: dispatch2 kernel + mm startup, / qjs whole process [E] |
|---|---|---|---|---|---|---|
| fib | 22.5 | 20.7 | 28.6 | 1.08 | 1.38 | 1.15 |
| closure | 36.4 | 39.2 | 64.9 | 0.93 | 1.65 | 0.98 |
| callfn | 25.5 | 36.8 | 42.9 | 0.69 | 1.17 | 0.77 |
| loopfn | 8.8 | 20.4 | 15.9 | 0.43 | 0.78 | 0.60 |
| loop | 51.4 | 67.4 | 89.6 | 0.76 | 1.33 | 0.80 |
| dictint | 35.1 | 90.8 | 46.5 | 0.39 | 0.51 | 0.43 |

## Unrelated edits, first loop: instruction change vs no edit [M]

| program | perturb_arm | perturb_order | perturb_field | perturb_cold | perturb_slow |
|---|---|---|---|---|---|
| fib | +0.50% | -0.00% | -0.00% | -0.00% | -0.00% |
| closure | +1.99% | +0.36% | -0.00% | -0.00% | -0.00% |
| callfn | -0.25% | -0.00% | -0.00% | -0.00% | -0.00% |
| loopfn | -0.00% | -0.00% | -0.00% | -0.00% | -0.00% |
| loop | +2.53% | +0.63% | -0.00% | -0.00% | -0.00% |
| dictint | +1.11% | +0.17% | -0.00% | -0.00% | -0.00% |

## Unrelated edits, final loop: instruction change vs no edit [M]

| program | perturb_arm | perturb_order | perturb_field | perturb_cold | perturb_slow |
|---|---|---|---|---|---|
| fib | +0.00% | +0.14% | +0.00% | -0.00% | -0.00% |
| closure | +0.00% | +0.19% | +0.00% | -0.00% | -0.00% |
| callfn | -0.00% | -0.00% | +0.00% | -0.00% | -0.00% |
| loopfn | -0.00% | -0.00% | +0.00% | -0.00% | -0.00% |
| loop | -0.00% | +0.35% | +0.00% | -0.00% | -0.00% |
| dictint | -0.00% | +0.06% | +0.00% | -0.00% | -0.00% |
