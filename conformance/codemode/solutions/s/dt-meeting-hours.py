from datetime import datetime, timezone

def parse(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt

events = await list_events({"start": "2026-03-16T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "alice"})
lo = datetime(2026, 3, 16, tzinfo=timezone.utc)
hi = datetime(2026, 3, 23, tzinfo=timezone.utc)
total = 0.0
count = 0
seen = set()
for e in events:
    if e["id"] in seen or "alice" not in (e.get("attendees") or []):
        continue
    st, en = parse(e["start"]), parse(e["end"])
    if not (lo <= st < hi):
        continue
    seen.add(e["id"])
    total += (en - st).total_seconds()
    count += 1
print(count, total / 3600)
await submit_answer({"hours": round(total / 3600, 2), "events": count})
