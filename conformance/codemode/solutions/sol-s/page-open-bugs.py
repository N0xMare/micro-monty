keys = []
page = 1
while True:
    result = await search_issues({"query": "label:bug", "page": page})
    keys.extend(issue["key"] for issue in result["issues"] if issue["status"] != "closed")
    if not result["has_more"]:
        break
    page += 1
await submit_answer({"keys": sorted(keys)})
