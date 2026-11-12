import datetime

keys = []
page_number = 1
while True:
    page = await search_issues({"query": "", "page": page_number})
    for issue in page["issues"]:
        created = datetime.datetime.fromisoformat(issue["created"].replace("Z", "+00:00"))
        if created.weekday() >= 5:
            keys.append(issue["key"])
    if not page["has_more"]:
        break
    page_number += 1
await submit_answer({"keys": sorted(keys)})
