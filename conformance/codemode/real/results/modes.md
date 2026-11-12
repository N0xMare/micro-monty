| family | composition | mode | sessions | answered | cells | failed | median engine ms | p90 engine ms | median host call ms | memory KiB (median per session) |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| codex | py-ptc | monty | 8 | 8 | 61 | 9 | 0.37 | 0.67 | 54.6 | 10,120 (worker peak) |
| codex | py-ptc | mm | 8 | 8 | 43 | 6 | 0.42 | 0.57 | 58.5 | 4,948 (worker peak) |
| codex | py-ptc | native | 8 | 8 | 42 | 8 | 0.32 | 0.45 | 55.0 | 7,712 (plak growth) |
| codex | py-ptc | wasm | 8 | 8 | 55 | 6 | 0.44 | 0.73 | 54.0 | 8,852 (plak growth) |
| codex | py-ptc | cache | 8 | 8 | 43 | 8 | 0.34 | 0.47 | 54.1 | 7,654 (plak growth) |
| codex | py-sdk | monty | 8 | 8 | 60 | 4 | 2.39 | 16.17 | 34.3 | 30,436 (worker peak) |
| codex | py-sdk | mm | 8 | 8 | 62 | 3 | 2.54 | 5.93 | 31.0 | 7,072 (worker peak) |
| codex | py-sdk | native | 8 | 8 | 61 | 4 | 2.33 | 4.71 | 33.6 | 12,718 (plak growth) |
| codex | py-sdk | wasm | 8 | 7 | 57 | 3 | 2.85 | 6.08 | 29.7 | 12,488 (plak growth) |
| codex | py-sdk | cache | 8 | 8 | 60 | 2 | 2.02 | 3.85 | 26.2 | 12,314 (plak growth) |
| grok | py-ptc | monty | 8 | 8 | 171 | 7 | 0.48 | 1.33 | 37.7 | 12,160 (worker peak) |
| grok | py-ptc | mm | 8 | 7 | 219 | 2 | 0.50 | 0.97 | 42.2 | 5,878 (worker peak) |
| grok | py-ptc | native | 8 | 7 | 173 | 4 | 0.40 | 0.64 | 37.0 | 16,256 (plak growth) |
| grok | py-ptc | wasm | 8 | 8 | 179 | 5 | 0.68 | 1.91 | 40.3 | 20,086 (plak growth) |
| grok | py-ptc | cache | 8 | 7 | 230 | 3 | 0.40 | 0.64 | 37.7 | 16,348 (plak growth) |
| grok | py-sdk | monty | 8 | 7 | 239 | 15 | 1.74 | 2.89 | 33.9 | 30,926 (worker peak) |
| grok | py-sdk | mm | 8 | 7 | 292 | 20 | 1.95 | 3.26 | 36.8 | 7,278 (worker peak) |
| grok | py-sdk | native | 8 | 8 | 256 | 12 | 1.76 | 2.29 | 32.5 | 20,378 (plak growth) |
| grok | py-sdk | wasm | 8 | 7 | 282 | 10 | 2.58 | 3.76 | 34.0 | 21,430 (plak growth) |
| grok | py-sdk | cache | 8 | 8 | 256 | 12 | 1.78 | 2.46 | 32.2 | 20,468 (plak growth) |
