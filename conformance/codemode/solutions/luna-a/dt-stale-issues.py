import json
from datetime import datetime, timezone, timedelta
cutoff = datetime(2026,3,15,12,tzinfo=timezone.utc)-timedelta(days=14); keys = set()
for status in ("open", "in_progress"):
    page = 1
    while True:
        r = json.loads(await search_issues({"query": f"status:{status}", "page": page}))
        for x in r["issues"]:
            if datetime.fromisoformat(x["updated"].replace("Z", "+00:00")) < cutoff: keys.add(x["key"])
        if not r["has_more"]: break
        page += 1
await submit_answer({"keys": sorted(keys)})
