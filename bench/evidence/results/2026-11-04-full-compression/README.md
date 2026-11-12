# Host compression of FULL snapshots (2026-11-04)

[M]. Images from main's engine (`fec4ff6`, `mm` SHA-256 `ec95bd9e…`), made by

```
~/.local/bin/python3.14 -I bench/evidence/compression.py <main mm> images.jsonl
```

90 images are the first park (`mm exec --snapshot-every 100`) of the
`conformance/own` cases that park; 3 are retained-state programs holding
1k/10k/100k dicts of four fields, parked in a loop after building them. Codecs
run single-threaded in-process, independent streams, no dictionary, best of 5.
Raw per-image results: `images.jsonl`.

| Images | raw | zlib-1 | zlib-6 | zstd-1 | zstd-3 |
|---|---:|---:|---:|---:|---:|
| 90 parked own cases (median) | 71,410 B | 3.3× (2.7–8.1) | 3.6× | 3.6× (2.7–9.3) | 3.6× |
| state, 1k rows | 173,475 B | 6.7× | 7.1× | 9.9× | 9.7× |
| state, 10k rows | 1,448,435 B | 9.2× | 9.7× | 17.5× | 17.0× |
| state, 100k rows | 13,886,167 B | 9.7× | 10.3× | 19.2× | 19.1× |

CPU at 100k rows (encode / decode): zlib-1 40.2 / 20.0 ms, zlib-6 171.6 / 17.6
ms, zstd-1 16.0 / 11.0 ms, zstd-3 20.1 / 11.1 ms. Median own case: zstd-1
0.18 / 0.09 ms, zlib-1 0.62 / 0.23 ms.

Recommendation: zstd level 1 at the storage boundary. Not measured: storage
latency, dictionary-trained zstd across a session's images, or cross-image
dedup.
