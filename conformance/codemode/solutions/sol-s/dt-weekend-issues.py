from datetime import datetime, timezone

keys = []
page_number = 1
while True:
    page = await search_issues({"query": "", "page": page_number})
    keys.extend(i["key"] for i in page["issues"] if datetime.fromisoformat(i["created"]).astimezone(timezone.utc).weekday() >= 5)
    if not page["has_more"]:
        break
    page_number += 1
await submit_answer({"keys": sorted(keys)})
