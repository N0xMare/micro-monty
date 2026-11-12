import asyncio, json

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "status:open label:security", "page": page}))
    issues.extend(res["issues"])
    if not res.get("has_more"):
        break
    page += 1

to_raise = sorted(
    i["key"] for i in issues
    if i["status"] == "open" and "security" in i["labels"] and i["priority"] in ("P3", "P4")
)
print("raising to P2:", to_raise)
await asyncio.gather(*(update_issue({"key": k, "fields": {"priority": "P2"}}) for k in to_raise))
print(await submit_answer({"changed": to_raise}))
