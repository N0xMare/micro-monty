from collections import Counter

counts = Counter()
seen = set()
page = 1
while True:
    resp = await search_issues({"query": "status:open", "page": page})
    for i in resp["issues"]:
        if i["status"] == "open" and i["key"] not in seen:
            seen.add(i["key"])
            counts[i["assignee"] or "unassigned"] += 1
    if not resp.get("has_more"):
        break
    page += 1
print(dict(counts))
await submit_answer({"counts": dict(counts)})
