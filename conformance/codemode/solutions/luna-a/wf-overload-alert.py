import json

issues = []
page_num = 1
while True:
    page = json.loads(await search_issues({"query": "assignee:bjorn", "page": page_num}))
    issues.extend(page["issues"])
    if not page["has_more"]:
        break
    page_num += 1
total = sum((i["estimate_hours"] or 0) for i in issues if i["assignee"] == "bjorn" and i["status"] != "closed")
if total > 20:
    await post_message({"channel": "#eng", "text": f"bjorn is overloaded ({total}h)"})
