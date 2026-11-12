import asyncio
import json
issues = []
page_number = 1
while True:
    page = json.loads(await search_issues({"query": "status:in_progress", "page": page_number}))
    issues.extend(page["issues"])
    if not page["has_more"]:
        break
    page_number += 1
async def close_if_fixed(issue):
    detail = json.loads(await get_issue({"key": issue["key"]}))
    comments = detail["comments"]
    if comments and comments[-1]["body"].startswith("Fixed in #"):
        await update_issue({"key": issue["key"], "fields": {"status": "closed"}})
        await add_comment({"key": issue["key"], "body": "Closing: fix merged"})
await asyncio.gather(*(close_if_fixed(issue) for issue in issues))
