import json

issues = []
page_number = 1
while True:
    result = json.loads(await search_issues({"query": "label:bug", "page": page_number}))
    issues.extend(issue["key"] for issue in result["issues"] if issue["status"] != "closed" and "bug" in issue["labels"])
    if not result["has_more"]:
        break
    page_number += 1
await submit_answer({"keys": sorted(issues)})
