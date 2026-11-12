import asyncio

async def all_issues(query):
    issues = []
    page = 1
    while True:
        result = await search_issues({"query": query, "page": page})
        issues.extend(result["issues"])
        if not result["has_more"]:
            return issues
        page += 1
team, issues = await asyncio.gather(list_team({}), all_issues(""))
loads = {member["user"]: 0 for member in team if member["on_call"]}
for issue in issues:
    if issue["status"] != "closed" and issue["assignee"] in loads:
        loads[issue["assignee"]] += 1
targets = sorted((issue for issue in issues if issue["status"] == "open" and issue["priority"] == "P1" and issue["assignee"] is None), key=lambda issue: issue["key"])
if targets and not loads:
    raise RuntimeError("No on-call team member available")
for issue in targets:
    username = min(loads, key=lambda user: (loads[user], user))
    await update_issue({"key": issue["key"], "fields": {"assignee": username}})
    loads[username] += 1
    await add_comment({"key": issue["key"], "body": f"Auto-assigned to {username}"})
