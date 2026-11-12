import json
from datetime import datetime, timezone

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "", "page": page}))
    issues += res["issues"]
    if not res.get("has_more"):
        break
    page += 1

def parse(s):
    d = datetime.fromisoformat(s)
    return d.astimezone(timezone.utc) if d.tzinfo else d

keys = sorted(i["key"] for i in issues if parse(i["created"]).weekday() >= 5)
print(len(issues), keys)
print(await submit_answer({"keys": keys}))
