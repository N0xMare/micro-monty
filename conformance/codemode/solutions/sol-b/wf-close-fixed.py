import asyncio
import json
issues = []
page = 1
while True:
    result = json.loads(await search_issues({"query": "status:in_progress", "page": page}))
    issues.extend(result["issues"])
    if not result["has_more"]:
        break
    page += 1
details = await asyncio.gather(*(get_issue({"key": issue["key"]}) for issue in issues))
async def close(issue):
    await update_issue({"key": issue["key"], "fields": {"status": "closed"}})
    await add_comment({"key": issue["key"], "body": "Closing: fix merged"})
targets = [issue for raw in details if (issue := json.loads(raw))["comments"] and issue["comments"][-1]["body"].startswith("Fixed in #")]
await asyncio.gather(*(close(issue) for issue in targets))
