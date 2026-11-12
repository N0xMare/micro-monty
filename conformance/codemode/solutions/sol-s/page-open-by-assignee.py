from collections import Counter

counts = Counter()
page = 1
while True:
    result = await search_issues({"query": "status:open", "page": page})
    counts.update(issue["assignee"] if issue["assignee"] is not None else "unassigned" for issue in result["issues"])
    if not result["has_more"]:
        break
    page += 1
await submit_answer({"counts": dict(counts)})
