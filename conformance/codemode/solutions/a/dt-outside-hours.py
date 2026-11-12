import json
import asyncio
from datetime import datetime, timezone, time
from zoneinfo import ZoneInfo

def parse(s):
    d = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if d.tzinfo is None:
        d = d.replace(tzinfo=timezone.utc)
    return d

profile, events = await asyncio.gather(
    get_user_profile({"user": "emi"}),
    list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "emi"}),
)
profile = json.loads(profile)
events = json.loads(events)
tz = ZoneInfo(profile["timezone"])
ws = time.fromisoformat(profile["working_hours"]["start"])
we = time.fromisoformat(profile["working_hours"]["end"])
lo = datetime(2026, 3, 9, tzinfo=timezone.utc)
hi = datetime(2026, 3, 23, tzinfo=timezone.utc)

out = []
for e in events:
    if "emi" not in e.get("attendees", []):
        continue
    s = parse(e["start"])
    if not (lo <= s < hi):
        continue
    t = s.astimezone(tz).time()
    if t < ws or t >= we:
        out.append((s, e["id"]))
out.sort(key=lambda r: r[0])
ids = [i for _, i in out]
print(profile, ids)
print(await submit_answer({"event_ids": ids}))
