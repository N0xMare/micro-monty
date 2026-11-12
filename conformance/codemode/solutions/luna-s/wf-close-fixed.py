import asyncio

issues = []
page = 1
while True:
    result = await search_issues({"query": "status:in_progress", "page": page})
    issues.extend(result["issues"])
    if not result["has_more"]: break
    page += 1
details = await asyncio.gather(*(get_issue({"key": issue["key"]}) for issue in issues))
targets = [item["key"] for item in details if item.get("comments") and item["comments"][-1]["body"].startswith("Fixed in #")]
for key in targets:
    await update_issue({"key": key, "fields": {"status": "closed"}})
    await add_comment({"key": key, "body": "Closing: fix merged"})
