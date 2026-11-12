import json
from datetime import datetime, timezone
items = json.loads(await list_deployments({}))
failed = [{"id": x["id"], "service": x["service"],
           "started": datetime.fromtimestamp(x["started_at"], timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
          for x in items if x["status"] == "failed"]
await submit_answer({"failed": failed})
