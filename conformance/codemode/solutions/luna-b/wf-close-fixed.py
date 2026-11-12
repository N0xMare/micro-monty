import json

issues, page = [], 1
while True:
    result = json.loads(await search_issues({"query": "status:in_progress", "page": page}))
    issues.extend(result["issues"])
    if not result["has_more"]: break
    page += 1
for issue in issues:
    full = json.loads(await get_issue({"key": issue["key"]}))
    comments = full.get("comments", [])
    if comments and comments[-1]["body"].startswith("Fixed in #"):
        await update_issue({"key": issue["key"], "fields": {"status": "closed"}})
        await add_comment({"key": issue["key"], "body": "Closing: fix merged"})
