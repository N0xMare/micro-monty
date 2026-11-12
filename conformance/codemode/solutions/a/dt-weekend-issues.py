import json
import re
from datetime import datetime, timezone

def parse(s):
    d = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if d.tzinfo is None:
        d = d.replace(tzinfo=timezone.utc)
    return d

issues = []
page = 1
while True:
    r = json.loads(await search_issues({"query": "", "page": page}))
    issues += r["issues"]
    if not r.get("has_more"):
        break
    page += 1

keys = [i["key"] for i in issues if parse(i["created"]).astimezone(timezone.utc).weekday() >= 5]
nat = lambda k: [int(t) if t.isdigit() else t for t in re.split(r"(\d+)", k)]
keys = sorted(set(keys), key=nat)
print(len(issues), keys)
print(await submit_answer({"keys": keys}))
