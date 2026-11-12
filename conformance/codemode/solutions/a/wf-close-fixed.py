import asyncio, json

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "status:in_progress", "page": page}))
    issues.extend(i for i in res["issues"] if i["status"] == "in_progress")
    if not res.get("has_more"):
        break
    page += 1

details = [json.loads(r) for r in await asyncio.gather(*(get_issue({"key": i["key"]}) for i in issues))]

closed = []
for d in details:
    comments = d.get("comments") or []
    if comments and comments[-1]["body"].startswith("Fixed in #"):
        await update_issue({"key": d["key"], "fields": {"status": "closed"}})
        await add_comment({"key": d["key"], "body": "Closing: fix merged"})
        closed.append(d["key"])

print(json.dumps({"closed": closed}))
if "submit_answer" in globals():
    await submit_answer({"closed": closed})
