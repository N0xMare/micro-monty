import json
from datetime import datetime
keys, page = set(), 1
while True:
    r = json.loads(await search_issues({"query": "", "page": page}))
    for x in r["issues"]:
        if datetime.fromisoformat(x["created"].replace("Z", "+00:00")).weekday() >= 5: keys.add(x["key"])
    if not r["has_more"]: break
    page += 1
await submit_answer({"keys": sorted(keys)})
