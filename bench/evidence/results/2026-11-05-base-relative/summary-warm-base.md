## Sizes (medians; [M])

| Images | n | FULL | heap same as base | segs same | Δb64 | Δb256 | Δb4k | Δseg | FULL+zstd1 | FULL+zstd1 (base prefix) | Δb64+zstd1 | Δb256+zstd1 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| own cases, first park | 117 | 79,504 | 21,232 (27%) | 0/240 | 59,618 | 60,841 | 68,469 | 79,556 | 22,333 | 20,609 | 16,407 | 16,588 |
| codemode, first park | 1736 | 102,818 | 22,072 (21%) | 0/3670 | 83,232 | 84,756 | 94,680 | 102,870 | 26,412 | 23,934 | 20,465 | 20,849 |
| codemode, last park | 1736 | 139,574 | 22,284 (16%) | 0/3720 | 119,061 | 120,810 | 131,438 | 139,626 | 33,915 | 28,992 | 28,258 | 28,450 |
| codemode, every park | 12991 | 98,983 | 21,744 (22%) | 0/27739 | 79,258 | 80,516 | 90,931 | 99,035 | 23,582 | 18,170 | 17,772 | 17,915 |

## Ratios against FULL raw (median of per-image ratios; [M])

| Images | Δb64 | Δb256 | FULL+zstd1 | FULL+zstd1 (base prefix) | Δb64+zstd1 | Δb256+zstd1 | prefix vs FULL+zstd1 | Δb64+zstd1 vs FULL+zstd1 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| own cases, first park | 1.3× | 1.3× | 3.6× | 4.0× | 4.7× | 4.7× | 1.10× | 1.36× |
| codemode, first park | 1.2× | 1.2× | 4.0× | 5.0× | 5.0× | 4.9× | 1.24× | 1.29× |
| codemode, last park | 1.2× | 1.2× | 4.1× | 5.0× | 5.0× | 5.0× | 1.20× | 1.20× |
| codemode, every park | 1.3× | 1.2× | 4.0× | 5.2× | 5.2× | 5.2× | 1.27× | 1.33× |

## Where the bytes are (medians; [M])

| Images | FULL | header | VM section | heap live | heap live same as base | base live |
|---|---:|---:|---:|---:|---:|---:|
| own cases, first park | 79,504 | 125 | 22,374 | 56,200 | 21,232 | 318,040 |
| codemode, first park | 102,818 | 125 | 19,621 | 86,096 | 22,072 | 318,040 |
| codemode, last park | 139,574 | 125 | 20,704 | 117,392 | 22,284 | 318,040 |
| codemode, every park | 98,983 | 125 | 20,244 | 81,992 | 21,744 | 318,040 |

## Behaviour ([M])

- program runs (own, bench, state): 183; swap output == plain output: 183; parks restored from base + delta: 89,711; bench checksum lines matched: 0/0
- codemode solutions: 1793; measured run == runner: 1793; swap run == runner: 1793; host calls: 8,490; parks measured: 12,991
