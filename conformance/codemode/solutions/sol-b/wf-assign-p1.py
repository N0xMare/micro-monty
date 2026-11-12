import asyncio
import json
async def all_issues(query):
    issues = []
    page = 1
    while True:
        result = json.loads(await search_issues({"query": query, "page": page}))
        issues.extend(result["issues"])
        if not result["has_more"]:
            return issues
        page += 1
raw_team, issues = await asyncio.gather(list_team({}), all_issues(""))
users = sorted(member["user"] for member in json.loads(raw_team) if member["on_call"])
loads = {user: sum(issue["assignee"] == user and issue["status"] != "closed" for issue in issues) for user in users}
targets = sorted((issue for issue in issues if issue["status"] == "open" and issue["priority"] == "P1" and issue["assignee"] is None), key=lambda issue: issue["key"])
for issue in targets:
    user = min(users, key=lambda user: (loads[user], user))
    await update_issue({"key": issue["key"], "fields": {"assignee": user}})
    loads[user] += 1
    await add_comment({"key": issue["key"], "body": f"Auto-assigned to {user}"})
