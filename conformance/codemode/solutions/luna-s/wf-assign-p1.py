import asyncio

team = await list_team({})
on_call = sorted(member["user"] for member in team if member["on_call"])

async def all_issues(query):
    found = []
    page = 1
    while True:
        result = await search_issues({"query": query, "page": page})
        found.extend(result["issues"])
        if not result["has_more"]: break
        page += 1
    return found

targets = [i for i in await all_issues("status:open priority:P1") if not i["assignee"]]
targets.sort(key=lambda issue: issue["key"])
loads = {}
for user in on_call:
    assigned = await all_issues(f"assignee:{user}")
    loads[user] = sum(issue["status"] != "closed" for issue in assigned)
for issue in targets:
    user = min(on_call, key=lambda name: (loads[name], name))
    await update_issue({"key": issue["key"], "fields": {"assignee": user}})
    await add_comment({"key": issue["key"], "body": f"Auto-assigned to {user}"})
    loads[user] += 1
