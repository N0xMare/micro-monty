import json
from collections import Counter

counts = Counter()
page_number = 1
while True:
    page = json.loads(await search_issues({"query": "status:open", "page": page_number}))
    counts.update(issue["assignee"] or "unassigned" for issue in page["issues"])
    if not page["has_more"]:
        break
    page_number += 1
await submit_answer({"counts": dict(counts)})
