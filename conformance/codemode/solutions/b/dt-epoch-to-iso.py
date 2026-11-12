import json
from datetime import datetime, timezone

deps = json.loads(await list_deployments({}))
failed = sorted((d for d in deps if d["status"] == "failed"), key=lambda d: d["started_at"])
out = [
    {
        "id": d["id"],
        "service": d["service"],
        "started": datetime.fromtimestamp(d["started_at"], tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    for d in failed
]
print(out)
print(await submit_answer({"failed": out}))
