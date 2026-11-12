import json

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "assignee:bjorn", "page": page}))
    issues.extend(res["issues"])
    if not res.get("has_more"):
        break
    page += 1

open_issues = [i for i in issues if i["assignee"] == "bjorn" and i["status"] != "closed"]
total = sum(i["estimate_hours"] or 0 for i in open_issues)
print([(i["key"], i["status"], i["estimate_hours"]) for i in open_issues], "total:", total)

if total > 20:
    print(await post_message({"channel": "#eng", "text": f"bjorn is overloaded ({total}h)"}))
else:
    print("not overloaded; nothing posted")
# submit_answer is not in this cell's tool catalog, so the result is reported via print only.
