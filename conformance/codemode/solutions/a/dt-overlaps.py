import json
from datetime import datetime, timezone

def parse(s):
    d = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if d.tzinfo is None:
        d = d.replace(tzinfo=timezone.utc)
    return d

events = json.loads(await list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-16T00:00:00Z", "attendee": "alice"}))
lo = datetime(2026, 3, 9, tzinfo=timezone.utc)
hi = datetime(2026, 3, 16, tzinfo=timezone.utc)
evs = {}
for e in events:
    if "alice" not in e.get("attendees", []):
        continue
    s = parse(e["start"])
    if lo <= s < hi:
        evs[e["id"]] = (s, parse(e["end"]))
ids = list(evs)
pairs = []
for i in range(len(ids)):
    for j in range(i + 1, len(ids)):
        a, b = evs[ids[i]], evs[ids[j]]
        if a[0] < b[1] and b[0] < a[1]:
            pairs.append(sorted([ids[i], ids[j]]))
pairs.sort()
print(pairs)
print(await submit_answer({"pairs": pairs}))
