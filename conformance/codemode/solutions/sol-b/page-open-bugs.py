import json

keys = []
page_number = 1
while True:
    page = json.loads(await search_issues({"query": "label:bug", "page": page_number}))
    keys.extend(issue["key"] for issue in page["issues"] if issue["status"] != "closed")
    if not page["has_more"]:
        break
    page_number += 1
await submit_answer({"keys": sorted(keys)})
