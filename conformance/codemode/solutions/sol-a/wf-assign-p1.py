import json
team = json.loads(await list_team({}))
loads = {member["user"]: 0 for member in team if member["on_call"]}
issues = []
page_number = 1
while True:
    page = json.loads(await search_issues({"query": "", "page": page_number}))
    issues.extend(page["issues"])
    if not page["has_more"]:
        break
    page_number += 1
for issue in issues:
    if issue["status"] != "closed" and issue["assignee"] in loads:
        loads[issue["assignee"]] += 1
targets = sorted((issue for issue in issues if issue["status"] == "open" and issue["priority"] == "P1" and issue["assignee"] is None), key=lambda issue: issue["key"])
if targets and not loads:
    raise RuntimeError("No on-call team members available")
for issue in targets:
    username = min(loads, key=lambda user: (loads[user], user))
    await update_issue({"key": issue["key"], "fields": {"assignee": username}})
    loads[username] += 1
    await add_comment({"key": issue["key"], "body": f"Auto-assigned to {username}"})
