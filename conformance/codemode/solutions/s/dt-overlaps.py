from datetime import datetime, timezone

def parse(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt

events = await list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-16T00:00:00Z", "attendee": "alice"})
lo = datetime(2026, 3, 9, tzinfo=timezone.utc)
hi = datetime(2026, 3, 16, tzinfo=timezone.utc)
evs = {}
for e in events:
    if "alice" not in (e.get("attendees") or []):
        continue
    st, en = parse(e["start"]), parse(e["end"])
    if lo <= st < hi:
        evs[e["id"]] = (st, en)
items = list(evs.items())
pairs = []
for i in range(len(items)):
    for j in range(i + 1, len(items)):
        (a, (s1, e1)), (b, (s2, e2)) = items[i], items[j]
        if s1 < e2 and s2 < e1:
            pairs.append(sorted([a, b]))
pairs.sort()
print(pairs)
await submit_answer({"pairs": pairs})
