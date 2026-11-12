"""Paired per-round time ratios against a reference build, from `measure.py
programs` sessions. Usage: program_ratios.py RESULTS_DIR [REFERENCE].
Pairs are samples of the same program and round (adjacent in time); the
interval is a bootstrap 95% CI of the median ratio, pooled over sessions."""
import json
import random
import statistics
import sys
from pathlib import Path

root = Path(sys.argv[1])
reference = sys.argv[2] if len(sys.argv) > 2 else "main"
random.seed(1)
rounds = {}  # (session, program, block) -> {variant: ms}
variants, programs = [], []
for session in sorted(root.glob("session-*/samples.jsonl")):
    for line in session.read_text().splitlines():
        s = json.loads(line)
        assert s["outcome"] == "pass", (session, s["program"], s["engine"], s["outcome"])
        if s["engine"] not in variants:
            variants.append(s["engine"])
        if s["program"] not in programs:
            programs.append(s["program"])
        if s["block"] >= 0:
            rounds.setdefault((session.parent.name, s["program"], s["block"]), {})[s["engine"]] = \
                s["process_ns"] / 1e6
others = [v for v in variants if v != reference]
print(f"| Program | {reference} ms | " + " | ".join(f"{v}/{reference} [95% CI]" for v in others) + " |")
print("|---|---:|" + "---:|" * len(others))
for program in programs:
    pairs = [r for (_, p, _), r in rounds.items() if p == program]
    cells = []
    for v in others:
        ratios = [r[v] / r[reference] for r in pairs]
        boots = sorted(statistics.median(random.choices(ratios, k=len(ratios))) for _ in range(2000))
        cells.append(f"{statistics.median(ratios):.3f} [{boots[50]:.3f}, {boots[1949]:.3f}]")
    base = statistics.median(r[reference] for r in pairs)
    print(f"| {program} | {base:.1f} | " + " | ".join(cells) + " |")
print(f"\npaired rounds per program: {len(pairs)}")
