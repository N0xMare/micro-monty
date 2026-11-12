import json
from datetime import datetime, timezone
deployments = json.loads(await list_deployments({}))
await submit_answer({"failed": [{"id": d["id"], "service": d["service"], "started": datetime.fromtimestamp(d["started_at"], timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")} for d in sorted(deployments, key=lambda d: d["started_at"]) if d["status"] == "failed"]})
