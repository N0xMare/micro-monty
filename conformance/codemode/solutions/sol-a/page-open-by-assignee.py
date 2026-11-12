import json
from collections import Counter

counts = Counter()
number = 1
while True:
    page = json.loads(await search_issues({"query": "status:open", "page": number}))
    counts.update(issue["assignee"] if issue["assignee"] is not None else "unassigned" for issue in page["issues"])
    if not page["has_more"]:
        break
    number = page["page"] + 1
await submit_answer({"counts": dict(counts)})
