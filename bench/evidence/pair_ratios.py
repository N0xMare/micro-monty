"""Paired candidate/main ratios per program from xtask bench samples.jsonl.
Pairs are the two engines' samples in the same round (adjacent in time)."""
import json, random, statistics, sys
from pathlib import Path

PROGRAMS = ["startup", "fib", "loop", "dict", "str", "json", "closure", "classes",
            "generators", "exceptions", "fmt", "sort", "interp"]
root = Path(sys.argv[1])
sessions = sorted(root.glob("session-*/samples.jsonl"))
random.seed(1)
rows = {p: [] for p in PROGRAMS}
med = {p: [] for p in PROGRAMS}
for path in sessions:
    samples = [json.loads(l) for l in path.read_text().splitlines()]
    assert all(s["error"] is None for s in samples), path
    by = {}
    for s in samples:
        if not s["warmup"]:
            by.setdefault((s["program"], s["round"]), {})[s["engine"]] = s["elapsed_ms"]
    for p in PROGRAMS:
        pairs = [v for (q, _), v in by.items() if q == p]
        ratios = [v["micro-monty"] / v["micro-monty-main"] for v in pairs]
        boots = sorted(statistics.median(random.choices(ratios, k=len(ratios))) for _ in range(2000))
        rows[p].append((statistics.median(ratios), boots[50], boots[1949], len(ratios)))
        med[p].append((statistics.median(v["micro-monty-main"] for v in pairs),
                       statistics.median(v["micro-monty"] for v in pairs)))
print("| Program | main ms | candidate ms | " + " | ".join(f"S{i+1} cand/main [95% CI]" for i in range(len(sessions))) + " |")
print("|---|---:|---:|" + "---:|" * len(sessions))
for p in PROGRAMS:
    m = statistics.median(x for x, _ in med[p]); c = statistics.median(y for _, y in med[p])
    cells = " | ".join(f"{r:.3f} [{lo:.3f}, {hi:.3f}]" for r, lo, hi, _ in rows[p])
    print(f"| {p} | {m:.1f} | {c:.1f} | {cells} |")
print("\nrounds per session:", rows["fib"][0][3])
