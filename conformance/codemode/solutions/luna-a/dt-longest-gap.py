import json
items = sorted(json.loads(await list_deployments({})), key=lambda x: x["started_at"])
gap, before, after = max((b["started_at"]-a["started_at"], a["id"], b["id"]) for a, b in zip(items, items[1:]))
await submit_answer({"hours": round(gap/3600, 2), "before": before, "after": after})
