import json
from datetime import datetime, timezone

issues, page = [], 1
while True:
    res = json.loads(await search_issues({"query": "", "page": page}))
    issues.extend(res["issues"])
    if not res["has_more"]:
        break
    page += 1

def parse(ts):
    dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
    return dt if dt.tzinfo else dt.replace(tzinfo=timezone.utc)

now = datetime(2026, 3, 15, 12, 0, 0, tzinfo=timezone.utc)
open_issues = [i for i in issues if i["status"] != "closed"]
by_prio = {p: sum(1 for i in open_issues if i["priority"] == p) for p in ["P1", "P2", "P3", "P4"]}
oldest = min(open_issues, key=lambda i: (parse(i["created"]), i["key"]))
days = int((now - parse(oldest["created"])).total_seconds() // 86400)

lines = [f"Open issues: {len(open_issues)}"]
lines += [f"  {p}: {n}" for p, n in by_prio.items()]
lines.append(f"Oldest: {oldest['key']} ({days} days) - {oldest['title']}")
text = "\n".join(lines)

print(text)
print(await post_message({"channel": "#eng", "text": text}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"text": text}))
