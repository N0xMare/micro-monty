import asyncio, json
from datetime import datetime, timedelta, timezone

people = ["alice", "bjorn", "diego"]
# Query a wider window so events that start earlier but overlap the afternoon are included.
results = await asyncio.gather(*[
    list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-19T00:00:00Z", "attendee": p})
    for p in people
])

def parse(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)

busy = []
seen = set()
for p, r in zip(people, results):
    for e in json.loads(r):
        if p not in e.get("attendees", []) or (e["id"], p) in seen:
            continue
        seen.add((e["id"], p))
        busy.append((parse(e["start"]), parse(e["end"]), p, e["title"]))

day_start = datetime(2026, 3, 18, 13, 0, tzinfo=timezone.utc)
day_end = datetime(2026, 3, 18, 17, 0, tzinfo=timezone.utc)
slot = None
t = day_start
while t + timedelta(minutes=60) <= day_end:
    end = t + timedelta(minutes=60)
    if not any(s < end and e > t for s, e, _, _ in busy):
        slot = (t, end)
        break
    t += timedelta(minutes=15)

print(sorted((s.isoformat(), e.isoformat(), p, title) for s, e, p, title in busy if s < day_end and e > day_start))
if slot is None:
    print("No free 60-minute slot between 13:00 and 17:00 UTC on 2026-03-18; no event created.")
else:
    res = await create_event({
        "title": "Sync",
        "start": slot[0].strftime("%Y-%m-%dT%H:%M:%S+00:00"),
        "end": slot[1].strftime("%Y-%m-%dT%H:%M:%S+00:00"),
        "attendees": ["alice", "bjorn", "diego"],
    })
    print("slot:", slot[0].isoformat(), "-", slot[1].isoformat(), res)
