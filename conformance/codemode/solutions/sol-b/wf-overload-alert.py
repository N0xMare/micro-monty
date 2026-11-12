import json
page = 1
total = 0
while True:
    result = json.loads(await search_issues({"query": "assignee:bjorn", "page": page}))
    total += sum(issue["estimate_hours"] or 0 for issue in result["issues"] if issue["status"] != "closed")
    if not result["has_more"]:
        break
    page += 1
if total > 20:
    await post_message({"channel": "#eng", "text": f"bjorn is overloaded ({total}h)"})
