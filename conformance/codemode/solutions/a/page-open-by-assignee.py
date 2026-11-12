import json
from collections import Counter

counts = Counter()
seen = set()
page = 1
while True:
    res = json.loads(await search_issues({"query": "status:open", "page": page}))
    for i in res["issues"]:
        if i["status"] == "open" and i["key"] not in seen:
            seen.add(i["key"])
            counts[i["assignee"] or "unassigned"] += 1
    if not res.get("has_more") or not res["issues"]:
        break
    page += 1
print(len(seen), dict(counts))
await submit_answer({"counts": dict(counts)})
