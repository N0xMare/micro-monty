import json
from datetime import datetime, timezone

rows = json.loads(await list_deployments({}))
failed = [{"id": d["id"], "service": d["service"], "started": datetime.fromtimestamp(d["started_at"], timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")} for d in rows if d["status"] == "failed"]
await submit_answer({"failed": failed})
