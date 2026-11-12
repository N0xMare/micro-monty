import json

deps = json.loads(await list_deployments({}))
deps.sort(key=lambda d: d["started_at"])
best = None
for a, b in zip(deps, deps[1:]):
    gap = b["started_at"] - a["started_at"]
    if best is None or gap > best[0]:
        best = (gap, a["id"], b["id"])
print(best)
gap, before, after = best
print(await submit_answer({"hours": round(gap / 3600, 2), "before": before, "after": after}))
