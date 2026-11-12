import asyncio
import json
issues = []
page = 1
while True:
    result = json.loads(await search_issues({"query": "status:open label:security", "page": page}))
    issues.extend(result["issues"])
    if not result["has_more"]:
        break
    page += 1
changed = sorted(issue["key"] for issue in issues if issue["priority"] in ("P3", "P4"))
await asyncio.gather(*(update_issue({"key": key, "fields": {"priority": "P2"}}) for key in changed))
await submit_answer({"changed": changed})
