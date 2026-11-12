issues, page = [], 1
while True:
    res = await search_issues({"query": "status:open label:security", "page": page})
    issues.extend(res["issues"])
    if not res.get("has_more"):
        break
    page += 1

changed = []
for i in issues:
    if i["status"] == "open" and "security" in (i.get("labels") or []) and i["priority"] in ("P3", "P4"):
        await update_issue({"key": i["key"], "fields": {"priority": "P2"}})
        changed.append(i["key"])

changed.sort()
print(changed)
await submit_answer({"changed": changed})
