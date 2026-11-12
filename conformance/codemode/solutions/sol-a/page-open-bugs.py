import json

keys = []
number = 1
while True:
    page = json.loads(await search_issues({"query": "label:bug", "page": number}))
    keys.extend(issue["key"] for issue in page["issues"] if issue["status"] != "closed")
    if not page["has_more"]:
        break
    number = page["page"] + 1
await submit_answer({"keys": sorted(keys)})
