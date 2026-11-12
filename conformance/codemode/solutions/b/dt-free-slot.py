import asyncio, json
from datetime import datetime, timedelta, timezone

people = ["alice", "bjorn", "diego"]
# Start the window a day early so events that began earlier and run into the afternoon are included.
results = await asyncio.gather(*(
    list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T17:00:00Z", "attendee": p})
    for p in people
))

def parse(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)

busy = []
seen = set()
for p, raw in zip(people, results):
    for e in json.loads(raw):
        if p not in e.get("attendees", []) or (e["id"], p) in seen:
            continue
        seen.add((e["id"], p))
        busy.append((parse(e["start"]), parse(e["end"]), p, e["title"]))
print(sorted(busy))

window_start = datetime(2026, 3, 18, 13, 0, tzinfo=timezone.utc)
window_end = datetime(2026, 3, 18, 17, 0, tzinfo=timezone.utc)
slot = None
t = window_start
while t + timedelta(minutes=60) <= window_end:
    end = t + timedelta(minutes=60)
    if all(not (s < end and t < e) for s, e, _, _ in busy):
        slot = (t, end)
        break
    t += timedelta(minutes=15)

print("slot:", slot)
if slot is None:
    result = {"error": "no free 60-minute slot between 13:00 and 17:00 UTC"}
else:
    fmt = lambda d: d.strftime("%Y-%m-%dT%H:%M:%S+00:00")
    created = json.loads(await create_event({
        "title": "Sync",
        "start": fmt(slot[0]),
        "end": fmt(slot[1]),
        "attendees": ["alice", "bjorn", "diego"],
    }))
    result = {"event_id": created["id"], "start": fmt(slot[0]), "end": fmt(slot[1])}
print(result)

# The task says to finish with submit_answer, but this prompt's catalog does not list it.
if "submit_answer" in globals():
    print(await submit_answer(result))
