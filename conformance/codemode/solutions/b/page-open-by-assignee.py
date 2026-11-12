import json
from collections import Counter

counts = Counter()
page_no = 1
while True:
    page = json.loads(await search_issues({"query": "status:open", "page": page_no}))
    for i in page["issues"]:
        if i["status"] == "open":
            counts[i["assignee"] or "unassigned"] += 1
    if not page.get("has_more"):
        break
    page_no += 1

counts = dict(counts)
print(counts)
print(await submit_answer({"counts": counts}))
