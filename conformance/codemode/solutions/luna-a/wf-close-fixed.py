import json

issues = []
page_num = 1
while True:
    page = json.loads(await search_issues({"query": "status:in_progress", "page": page_num}))
    issues.extend(page["issues"])
    if not page["has_more"]:
        break
    page_num += 1
for issue in issues:
    if issue["status"] != "in_progress":
        continue
    full = json.loads(await get_issue({"key": issue["key"]}))
    comments = full.get("comments", [])
    if comments and comments[-1]["body"].startswith("Fixed in #"):
        await update_issue({"key": issue["key"], "fields": {"status": "closed"}})
        await add_comment({"key": issue["key"], "body": "Closing: fix merged"})
