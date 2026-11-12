# Restore cost per MiB of image

Not a gate. The evidence behind the restore figures in docs/sdk.md ("A
snapshot or restore is not a step") and docs/spec.md §9.3: snapshot and
restore time per MiB of image, natively, under wasmtime and on V8, so a
thread can rerun them on any commit, such as a release candidate.

```bash
bench/evidence/restore-cost/run.sh        # 3 passes; run.sh N for N
```

`run.sh` builds `main.rs` against the unchanged facade, natively and for
`wasm32-wasip1` (its own workspace and `target/`, so nothing enters the
engine's build or size budget), then runs the program natively, under
wasmtime (`$WASMTIME`, `~/.local/bin` or `PATH`) and on V8 (`node
--no-liftoff` with `node:wasi`, `run.mjs`). A missing runtime is skipped.
`summarize.py` prints one Markdown table per target with the range over
passes. Run it on an otherwise idle machine: it measures wall time.

## Images

- **`lists N`**: N two-int lists (`[[i, i + 1] for i in range(N)]`, N
  from 600k to 3M), the cell parked on fuel in a loop after building
  them. The small-object case, as the snapshot cost was measured.
- **`dumps finished`**: `json.dumps` of 5M ints, finished, so the image
  holds one large block (the text) beside the list.
- **`dumps parked`**: the same `json.dumps` parked at every 256k-unit
  fuel slice, each park saved and restored. Mid-dumps, the text grows in
  place with room reserved past its data; the image records that room,
  and restore reserves it again without filling it (4676c0a). The row
  gives the fastest and slowest restore per MiB over the parks.

## Columns

Per image, in ms per MiB of image: the best snapshot of 3, the best
restore of 5, and the first restore in the process for that image, which
is what a fresh worker pays. The first row's first restore also holds
V8's first-call warm-up. The `restore ms` column is the best restore in
ms.

## Results

docs/sdk.md and docs/spec.md §9.3 hold the published numbers [M], with
the commit and machine they were measured on. Orbs share their host, so
expect ranges rather than single values, and compare runs from the same
machine.
