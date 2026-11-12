import json
from datetime import datetime, timezone

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "", "page": page}))
    issues += res["issues"]
    if not res["has_more"]:
        break
    page += 1

def parse(ts):
    dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
    return dt if dt.tzinfo else dt.replace(tzinfo=timezone.utc)

now = datetime(2026, 3, 15, 12, 0, 0, tzinfo=timezone.utc)
open_issues = [i for i in issues if i["status"] != "closed"]
lines = [f"Open issues: {len(open_issues)}"]
for p in ["P1", "P2", "P3", "P4"]:
    lines.append(f"  {p}: {sum(1 for i in open_issues if i['priority'] == p)}")
if open_issues:
    oldest = min(open_issues, key=lambda i: (parse(i["created"]), i["key"]))
    days = (now - parse(oldest["created"])).days
    lines.append(f"Oldest: {oldest['key']} ({days} days) - {oldest['title']}")
text = "\n".join(lines)
print(text)
print(await post_message({"channel": "#eng", "text": text}))
