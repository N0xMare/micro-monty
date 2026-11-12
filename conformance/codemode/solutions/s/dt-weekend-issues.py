import re
from datetime import datetime, timezone

def parse(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)

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

keys = sorted((k for k, i in issues.items() if parse(i["created"]).weekday() >= 5), key=natkey)
print(keys)
await submit_answer({"keys": keys})
