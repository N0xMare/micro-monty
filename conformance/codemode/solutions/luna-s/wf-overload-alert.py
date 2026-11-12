issues = []
page = 1
while True:
    result = await search_issues({"query": "assignee:bjorn", "page": page})
    issues.extend(result["issues"])
    if not result["has_more"]: break
    page += 1
hours = sum((issue["estimate_hours"] or 0) for issue in issues if issue["status"] != "closed")
if hours > 20:
    await post_message({"channel": "#eng", "text": f"bjorn is overloaded ({hours}h)"})
