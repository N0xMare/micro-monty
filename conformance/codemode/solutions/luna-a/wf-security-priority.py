import json

changed = []
page_num = 1
while True:
    page = json.loads(await search_issues({"query": "status:open label:security", "page": page_num}))
    for issue in page["issues"]:
        if issue["status"] == "open" and "security" in issue["labels"] and issue["priority"] in ("P3", "P4"):
            await update_issue({"key": issue["key"], "fields": {"priority": "P2"}})
            changed.append(issue["key"])
    if not page["has_more"]:
        break
    page_num += 1
await submit_answer({"changed": sorted(changed)})
