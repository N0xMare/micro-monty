import json
from datetime import datetime, timedelta
cutoff = datetime.fromisoformat("2026-03-15T12:00:00Z") - timedelta(days=14)
keys, page = [], 1
while True:
    result = json.loads(await search_issues({"query": "", "page": page}))
    keys.extend(i["key"] for i in result["issues"] if i["status"] != "closed" and datetime.fromisoformat(i["updated"]) < cutoff)
    if not result["has_more"]:
        break
    page += 1
await submit_answer({"keys": sorted(keys)})
