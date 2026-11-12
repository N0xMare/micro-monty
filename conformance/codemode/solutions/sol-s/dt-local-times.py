import asyncio
from datetime import datetime
from zoneinfo import ZoneInfo

events, profile = await asyncio.gather(list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T00:00:00Z", "attendee": "chitra"}), get_user_profile({"user": "chitra"}))
zone = ZoneInfo(profile["timezone"])
events.sort(key=lambda e: datetime.fromisoformat(e["start"]))
await submit_answer({"events": [{"id": e["id"], "local_start": datetime.fromisoformat(e["start"]).astimezone(zone).strftime("%Y-%m-%d %H:%M")} for e in events]})
