import json
deployments = sorted(json.loads(await list_deployments({})), key=lambda d: d["started_at"])
before, after = max(zip(deployments, deployments[1:]), key=lambda pair: pair[1]["started_at"] - pair[0]["started_at"])
await submit_answer({"hours": round((after["started_at"] - before["started_at"]) / 3600, 2), "before": before["id"], "after": after["id"]})
