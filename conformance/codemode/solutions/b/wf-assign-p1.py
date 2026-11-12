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
print("on call:", on_call)

assigned_lists = await asyncio.gather(*(search_all(f"assignee:{u}") for u in on_call))
load = {
    u: sum(1 for i in issues if i["assignee"] == u and i["status"] != "closed")
    for u, issues in zip(on_call, assigned_lists)
}
print("initial load:", load)

candidates = await search_all("status:open priority:P1 assignee:None")
targets = sorted(
    (i for i in candidates if i["status"] == "open" and i["priority"] == "P1" and i["assignee"] is None),
    key=lambda i: i["key"],
)
print("unassigned open P1:", [i["key"] for i in targets])

assignments = {}
for issue in targets:
    user = min(on_call, key=lambda u: (load[u], u))
    load[user] += 1
    assignments[issue["key"]] = user
    await update_issue({"key": issue["key"], "fields": {"assignee": user}})
    await add_comment({"key": issue["key"], "body": f"Auto-assigned to {user}"})
    print(f"{issue['key']} -> {user}")

print("assignments:", assignments)
print("final load:", load)
# submit_answer is not in this cell's tool catalog, so the result is reported via print only.
