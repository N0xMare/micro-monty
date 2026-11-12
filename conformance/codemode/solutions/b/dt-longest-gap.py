import json

deps = json.loads(await list_deployments({}))
deps.sort(key=lambda d: d["started_at"])

best = None
for a, b in zip(deps, deps[1:]):
    gap = b["started_at"] - a["started_at"]
    if best is None or gap > best[0]:
        best = (gap, a["id"], b["id"])

gap, before, after = best
answer = {"hours": round(gap / 3600, 2), "before": before, "after": after}
print(len(deps), answer)
print(await submit_answer(answer))
