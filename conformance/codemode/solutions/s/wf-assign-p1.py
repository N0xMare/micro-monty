import asyncio

async def search_all(query):
    out, page = [], 1
    while True:
        res = await search_issues({"query": query, "page": page})
        out.extend(res["issues"])
        if not res.get("has_more"):
            return out
        page += 1

team, all_issues = await asyncio.gather(list_team({}), search_all(""))

on_call = sorted(m["user"] for m in team if m.get("on_call"))
load = {u: 0 for u in on_call}
for issue in all_issues:
    if issue["status"] != "closed" and issue.get("assignee") in load:
        load[issue["assignee"]] += 1
print("initial load:", load)

targets = sorted(
    (i for i in all_issues
     if i["status"] == "open" and i["priority"] == "P1" and not i.get("assignee")),
    key=lambda i: i["key"],
)

assignments = []
if on_call:
    for issue in targets:
        user = min(on_call, key=lambda u: (load[u], u))
        await update_issue({"key": issue["key"], "fields": {"assignee": user}})
        await add_comment({"key": issue["key"], "body": f"Auto-assigned to {user}"})
        load[user] += 1
        assignments.append({"key": issue["key"], "assignee": user})
print("assignments:", assignments)
print("final load:", load)

if "submit_answer" in globals():
    await submit_answer({"assignments": assignments})
