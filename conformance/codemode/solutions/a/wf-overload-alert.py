import json

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "assignee:bjorn", "page": page}))
    issues.extend(res["issues"])
    if not res.get("has_more"):
        break
    page += 1

total = sum((i["estimate_hours"] or 0) for i in issues if i["assignee"] == "bjorn" and i["status"] != "closed")
posted = None
if total > 20:
    posted = f"bjorn is overloaded ({total}h)"
    await post_message({"channel": "#eng", "text": posted})

print(json.dumps({"total": total, "posted": posted}))
if "submit_answer" in globals():
    await submit_answer({"total": total, "posted": posted is not None})
