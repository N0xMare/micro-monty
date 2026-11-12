import re
from datetime import datetime, timezone, timedelta

def parse(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt

def natkey(k):
    return [int(t) if t.isdigit() else t for t in re.split(r"(\d+)", k)]

issues = {}
page = 1
while True:
    r = await search_issues({"query": "", "page": page})
    for i in r["issues"]:
        issues[i["key"]] = i
    if not r.get("has_more"):
        break
    page += 1

now = datetime(2026, 3, 15, 12, 0, 0, tzinfo=timezone.utc)
keys = [k for k, i in issues.items()
        if i["status"] != "closed" and now - parse(i["updated"]) > timedelta(days=14)]
keys.sort(key=natkey)
print(keys)
await submit_answer({"keys": keys})
