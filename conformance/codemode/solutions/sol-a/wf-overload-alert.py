import json
total = 0
page_number = 1
while True:
    page = json.loads(await search_issues({"query": "assignee:bjorn", "page": page_number}))
    total += sum(issue["estimate_hours"] or 0 for issue in page["issues"] if issue["status"] != "closed")
    if not page["has_more"]:
        break
    page_number += 1
if total > 20:
    await post_message({"channel": "#eng", "text": f"bjorn is overloaded ({total}h)"})
