import json
from collections import Counter

counts = Counter()
page_number = 1
while True:
    result = json.loads(await search_issues({"query": "status:open", "page": page_number}))
    for issue in result["issues"]:
        if issue["status"] == "open":
            counts[issue["assignee"] or "unassigned"] += 1
    if not result["has_more"]:
        break
    page_number += 1
await submit_answer({"counts": dict(counts)})
