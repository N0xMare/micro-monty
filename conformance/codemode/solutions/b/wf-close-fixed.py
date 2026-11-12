import asyncio, json

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "status:in_progress", "page": page}))
    issues.extend(res["issues"])
    if not res.get("has_more"):
        break
    page += 1
issues = [i for i in issues if i["status"] == "in_progress"]

details = [json.loads(d) for d in await asyncio.gather(*(get_issue({"key": i["key"]}) for i in issues))]
to_close = sorted(
    d["key"] for d in details
    if d.get("comments") and d["comments"][-1]["body"].startswith("Fixed in #")
)
print("in_progress:", [d["key"] for d in details])
print("closing:", to_close)

for key in to_close:
    await update_issue({"key": key, "fields": {"status": "closed"}})
    await add_comment({"key": key, "body": "Closing: fix merged"})

print({"closed": to_close})
# submit_answer is not in this cell's tool catalog, so the result is reported via print only.
