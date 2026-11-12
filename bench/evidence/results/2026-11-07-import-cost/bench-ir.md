# Bench Ir: 1c03cff against this pass

`python3 bench/callgrind.py --engine base="/tmp/ic/b1/mm run" --engine new="/tmp/ic/b2/mm run" --keep DIR`,
whole-process Ir; `b1/mm` is `mm` built at 1c03cff, `b2/mm` this pass, both at
paths of equal length (glibc's start-up reads `/proc/self/maps`, whose
lines carry the binary's path) [M].

| Program | 1c03cff | this pass | change |
|---|---:|---:|---:|
| startup | 895,415 | 893,348 | -0.231% |
| fib | 321,649,745 | 321,647,761 | -0.001% |
| loop | 1,115,428,698 | 1,115,426,605 | -0.000% |
| dict | 1,161,157,807 | 1,161,155,814 | -0.000% |
| str | 493,447,272 | 493,445,390 | -0.000% |
| json | 1,131,695,671 | 1,131,347,642 | -0.031% |
| closure | 715,062,492 | 715,060,626 | -0.000% |
| classes | 1,382,456,618 | 1,382,044,647 | -0.030% |
| generators | 2,382,199,001 | 2,382,179,307 | -0.001% |
| exceptions | 991,783,501 | 991,902,764 | +0.012% |
| fmt | 1,561,402,149 | 1,561,393,169 | -0.001% |
| sort | 1,453,383,374 | 1,453,381,826 | -0.000% |
| interp | 2,277,356,345 | 2,277,358,045 | +0.000% |
