import datetime

cutoff = datetime.datetime.fromisoformat("2026-03-01T12:00:00+00:00")
keys = []
page_number = 1
while True:
    page = await search_issues({"query": "", "page": page_number})
    for issue in page["issues"]:
        updated = datetime.datetime.fromisoformat(issue["updated"].replace("Z", "+00:00"))
        if issue["status"] != "closed" and updated < cutoff:
            keys.append(issue["key"])
    if not page["has_more"]:
        break
    page_number += 1
await submit_answer({"keys": sorted(keys)})
