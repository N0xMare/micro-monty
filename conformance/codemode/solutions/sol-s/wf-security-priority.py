issues = []
page = 1
while True:
    result = await search_issues({"query": "status:open label:security", "page": page})
    issues.extend(result["issues"])
    if not result["has_more"]:
        break
    page += 1
changed = []
for issue in sorted(issues, key=lambda issue: issue["key"]):
    if issue["priority"] in ("P3", "P4"):
        await update_issue({"key": issue["key"], "fields": {"priority": "P2"}})
        changed.append(issue["key"])
await submit_answer({"changed": changed})
