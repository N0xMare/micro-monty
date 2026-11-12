import json
from datetime import datetime, timedelta, timezone

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "", "page": page}))
    issues += res["issues"]
    if not res.get("has_more"):
        break
    page += 1

now = datetime(2026, 3, 15, 12, 0, 0, tzinfo=timezone.utc)

def parse(s):
    d = datetime.fromisoformat(s)
    return d if d.tzinfo else d.replace(tzinfo=timezone.utc)

keys = sorted(
    i["key"] for i in issues
    if i["status"] != "closed" and now - parse(i["updated"]) > timedelta(days=14)
)
print(len(issues), keys)
print(await submit_answer({"keys": keys}))
