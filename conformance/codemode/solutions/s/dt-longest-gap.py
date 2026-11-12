deps = await list_deployments({})
deps = sorted(deps, key=lambda d: d["started_at"])
best = None
for a, b in zip(deps, deps[1:]):
    gap = b["started_at"] - a["started_at"]
    if best is None or gap > best[0]:
        best = (gap, a["id"], b["id"])
print(best)
await submit_answer({"hours": round(best[0] / 3600, 2), "before": best[1], "after": best[2]})
