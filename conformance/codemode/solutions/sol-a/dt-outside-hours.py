import asyncio
import json
from datetime import datetime, time
from zoneinfo import ZoneInfo

events_text, profile_text = await asyncio.gather(list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "emi"}), get_user_profile({"user": "emi"}))
profile = json.loads(profile_text)
zone = ZoneInfo(profile["timezone"])
start = time.fromisoformat(profile["working_hours"]["start"])
end = time.fromisoformat(profile["working_hours"]["end"])
events = sorted(json.loads(events_text), key=lambda e: datetime.fromisoformat(e["start"]))
ids = [e["id"] for e in events if not start <= datetime.fromisoformat(e["start"]).astimezone(zone).time() < end]
await submit_answer({"event_ids": ids})
