import asyncio
import json

team = json.loads(await list_team({}))
on_call = sorted({m["user"] for m in team if m["on_call"]})
issues, page = [], 1
while True:
    result = json.loads(await search_issues({"query": "status:open priority:P1", "page": page}))
    issues.extend(i for i in result["issues"] if i["assignee"] is None)
    if not result["has_more"]: break
    page += 1

async def load(user):
    found, p = [], 1
    while True:
        result = json.loads(await search_issues({"query": f"assignee:{user}", "page": p}))
        found.extend(result["issues"])
        if not result["has_more"]: break
        p += 1
    return user, sum(i["status"] != "closed" for i in found)

loads = dict(await asyncio.gather(*(load(user) for user in on_call)))
for issue in sorted(issues, key=lambda i: i["key"]):
    user = min(on_call, key=lambda u: (loads[u], u))
    await update_issue({"key": issue["key"], "fields": {"assignee": user}})
    await add_comment({"key": issue["key"], "body": f"Auto-assigned to {user}"})
    loads[user] += 1
