import json
from datetime import datetime, timezone

def parse(s):
    d = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if d.tzinfo is None:
        d = d.replace(tzinfo=timezone.utc)
    return d

events = json.loads(await list_events({"start": "2026-03-16T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "alice"}))
lo = datetime(2026, 3, 16, tzinfo=timezone.utc)
hi = datetime(2026, 3, 23, tzinfo=timezone.utc)
seen = set()
total = 0.0
count = 0
for e in events:
    if e["id"] in seen or "alice" not in e.get("attendees", []):
        continue
    s = parse(e["start"])
    if not (lo <= s < hi):
        continue
    seen.add(e["id"])
    total += (parse(e["end"]) - s).total_seconds()
    count += 1
print(count, total / 3600)
print(await submit_answer({"hours": round(total / 3600, 2), "events": count}))
