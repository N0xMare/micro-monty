from datetime import datetime, timedelta

cutoff = datetime.fromisoformat("2026-03-15T12:00:00Z") - timedelta(days=14)
keys = []
page_number = 1
while True:
    page = await search_issues({"query": "", "page": page_number})
    keys.extend(i["key"] for i in page["issues"] if i["status"] != "closed" and datetime.fromisoformat(i["updated"]) < cutoff)
    if not page["has_more"]:
        break
    page_number += 1
await submit_answer({"keys": sorted(keys)})
