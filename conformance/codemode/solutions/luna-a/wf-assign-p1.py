import json

team = json.loads(await list_team())
on_call = sorted(m["user"] for m in team if m["on_call"])
def pages(query):
    page_num = 1
    result = []
    while True:
        page = json.loads(await search_issues({"query": query, "page": page_num}))
        result.extend(page["issues"])
        if not page["has_more"]:
            break
        page_num += 1
    return result

all_issues = await pages("")
loads = {user: sum(1 for i in all_issues if i["assignee"] == user and i["status"] != "closed") for user in on_call}
candidates = [i for i in await pages("status:open priority:P1") if i["status"] == "open" and i["priority"] == "P1" and i["assignee"] is None]
for issue in sorted(candidates, key=lambda i: i["key"]):
    user = min(on_call, key=lambda u: (loads[u], u))
    await update_issue({"key": issue["key"], "fields": {"assignee": user}})
    await add_comment({"key": issue["key"], "body": f"Auto-assigned to {user}"})
    loads[user] += 1
