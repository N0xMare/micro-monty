import json, asyncio
from datetime import datetime, time
from zoneinfo import ZoneInfo

events_raw, profile_raw = await asyncio.gather(
    list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "emi"}),
    get_user_profile({"user": "emi"}),
)
events = [e for e in json.loads(events_raw) if "emi" in e.get("attendees", [])]
profile = json.loads(profile_raw)
tz = ZoneInfo(profile["timezone"])
ws = time.fromisoformat(profile["working_hours"]["start"])
we = time.fromisoformat(profile["working_hours"]["end"])

ids = []
for e in sorted(events, key=lambda e: datetime.fromisoformat(e["start"])):
    t = datetime.fromisoformat(e["start"]).astimezone(tz).time()
    if t < ws or t >= we:
        ids.append(e["id"])

print(profile["timezone"], ws, we, ids)
print(await submit_answer({"event_ids": ids}))
