import asyncio
from datetime import datetime, time
from zoneinfo import ZoneInfo

events, profile = await asyncio.gather(list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "emi"}), get_user_profile({"user": "emi"}))
zone = ZoneInfo(profile["timezone"])
start = time.fromisoformat(profile["working_hours"]["start"])
end = time.fromisoformat(profile["working_hours"]["end"])
events.sort(key=lambda e: datetime.fromisoformat(e["start"]))
ids = []
for event in events:
    local = datetime.fromisoformat(event["start"]).astimezone(zone).time()
    if local < start or local >= end:
        ids.append(event["id"])
await submit_answer({"event_ids": ids})
