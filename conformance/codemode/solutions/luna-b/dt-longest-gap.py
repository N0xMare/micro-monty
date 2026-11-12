import json

rows = json.loads(await list_deployments({}))
before, after = max(zip(rows, rows[1:]), key=lambda p: p[1]["started_at"] - p[0]["started_at"])
hours = round((after["started_at"] - before["started_at"]) / 3600, 2)
await submit_answer({"hours": hours, "before": before["id"], "after": after["id"]})
