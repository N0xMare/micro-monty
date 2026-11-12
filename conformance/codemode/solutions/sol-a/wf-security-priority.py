import json
issues = []
page_number = 1
while True:
    page = json.loads(await search_issues({"query": "status:open label:security", "page": page_number}))
    issues.extend(page["issues"])
    if not page["has_more"]:
        break
    page_number += 1
changed = []
for issue in issues:
    if issue["priority"] in {"P3", "P4"}:
        await update_issue({"key": issue["key"], "fields": {"priority": "P2"}})
        changed.append(issue["key"])
await submit_answer({"changed": sorted(changed)})
