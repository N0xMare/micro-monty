import json
from datetime import datetime, timezone
keys, page = [], 1
while True:
    result = json.loads(await search_issues({"query": "", "page": page}))
    keys.extend(i["key"] for i in result["issues"] if datetime.fromisoformat(i["created"]).astimezone(timezone.utc).weekday() >= 5)
    if not result["has_more"]:
        break
    page += 1
await submit_answer({"keys": sorted(keys)})
