issues = []
page = 1
while True:
    result = await search_issues({"query": "status:in_progress", "page": page})
    issues.extend(result["issues"])
    if not result["has_more"]:
        break
    page += 1
for summary in issues:
    issue = await get_issue({"key": summary["key"]})
    comments = issue["comments"]
    if issue["status"] == "in_progress" and comments and comments[-1]["body"].startswith("Fixed in #"):
        await update_issue({"key": issue["key"], "fields": {"status": "closed"}})
        await add_comment({"key": issue["key"], "body": "Closing: fix merged"})
