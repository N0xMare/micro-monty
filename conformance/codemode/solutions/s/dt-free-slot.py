import asyncio
from datetime import datetime, timedelta, timezone

people = ["alice", "bjorn", "diego"]
# Wide query window so events that start earlier but run into the afternoon are caught.
results = await asyncio.gather(*(
    list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T17:00:00Z", "attendee": p})
    for p in people
))

def parse(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)

busy = []
seen = set()
for evs in results:
    for e in evs:
        if e["id"] in seen:
            continue
        if not set(e.get("attendees", [])) & set(people):
            continue
        seen.add(e["id"])
        busy.append((parse(e["start"]), parse(e["end"]), e["title"]))
busy.sort()
print(busy)

day_start = datetime(2026, 3, 18, 13, 0, tzinfo=timezone.utc)
day_end = datetime(2026, 3, 18, 17, 0, tzinfo=timezone.utc)
slot = None
t = day_start
while t + timedelta(minutes=60) <= day_end:
    end = t + timedelta(minutes=60)
    if all(not (s < end and t < e) for s, e, _ in busy):
        slot = (t, end)
        break
    t += timedelta(minutes=15)

if slot is None:
    print("No free 60-minute slot between 13:00 and 17:00 UTC on 2026-03-18; nothing created.")
else:
    fmt = lambda d: d.strftime("%Y-%m-%dT%H:%M:%S+00:00")
    res = await create_event({
        "title": "Sync",
        "start": fmt(slot[0]),
        "end": fmt(slot[1]),
        "attendees": ["alice", "bjorn", "diego"],
    })
    print("Created:", fmt(slot[0]), "-", fmt(slot[1]), res)
