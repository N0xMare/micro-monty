from datetime import datetime, timezone

issues = []
page = 1
while True:
    r = await search_issues({"query": "", "page": page})
    issues.extend(r["issues"])
    if not r["has_more"]:
        break
    page += 1

open_issues = [i for i in issues if i["status"] != "closed"]

def parse(ts):
    dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt

now = datetime(2026, 3, 15, 12, 0, 0, tzinfo=timezone.utc)
by_prio = {p: 0 for p in ["P1", "P2", "P3", "P4"]}
for i in open_issues:
    if i["priority"] in by_prio:
        by_prio[i["priority"]] += 1

lines = [f"Open issues: {len(open_issues)}"]
lines += [f"  {p}: {n}" for p, n in by_prio.items()]
if open_issues:
    oldest = min(open_issues, key=lambda i: (parse(i["created"]), i["key"]))
    days = (now - parse(oldest["created"])).days
    lines.append(f"Oldest: {oldest['key']} ({days} days) - {oldest['title']}")
text = "\n".join(lines)
print(text)

res = await post_message({"channel": "#eng", "text": text})
print(res)
