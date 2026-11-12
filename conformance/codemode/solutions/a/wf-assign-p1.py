import asyncio, json

async def search_all(query):
    issues, page = [], 1
    while True:
        res = json.loads(await search_issues({"query": query, "page": page}))
        issues.extend(res["issues"])
        if not res.get("has_more"):
            return issues
        page += 1

team = json.loads(await list_team({}))
on_call = sorted(m["user"] for m in team if m["on_call"])

unassigned = [
    i for i in await search_all("status:open priority:P1 assignee:None")
    if i["status"] == "open" and i["priority"] == "P1" and not i["assignee"]
]
unassigned.sort(key=lambda i: i["key"])

loads_lists = await asyncio.gather(*(search_all(f"assignee:{u}") for u in on_call))
load = {
    u: sum(1 for i in issues if i["assignee"] == u and i["status"] != "closed")
    for u, issues in zip(on_call, loads_lists)
}

assignments = {}
if on_call:
    for issue in unassigned:
        user = min(on_call, key=lambda u: (load[u], u))
        await update_issue({"key": issue["key"], "fields": {"assignee": user}})
        await add_comment({"key": issue["key"], "body": f"Auto-assigned to {user}"})
        load[user] += 1
        assignments[issue["key"]] = user

print(json.dumps({"assignments": assignments, "on_call": on_call}))
if "submit_answer" in globals():
    await submit_answer({"assignments": assignments})
