import asyncio, json
from datetime import datetime
from zoneinfo import ZoneInfo

profile_raw, events_raw = await asyncio.gather(get_user_profile({"user": "emi"}), list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "emi"}))
profile = json.loads(profile_raw)
tz = ZoneInfo(profile["timezone"])
start, end = profile["working_hours"]["start"], profile["working_hours"]["end"]
outside = []
for e in json.loads(events_raw):
    local = datetime.fromisoformat(e["start"].replace("Z", "+00:00")).astimezone(tz)
    hm = local.strftime("%H:%M")
    if hm < start or hm >= end:
        outside.append(e["id"])
await submit_answer({"event_ids": outside})
