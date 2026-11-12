import asyncio, json
from datetime import datetime
from zoneinfo import ZoneInfo

profile_raw, events_raw = await asyncio.gather(get_user_profile({"user": "chitra"}), list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T00:00:00Z", "attendee": "chitra"}))
tz = ZoneInfo(json.loads(profile_raw)["timezone"])
events = json.loads(events_raw)
out = [{"id": e["id"], "local_start": datetime.fromisoformat(e["start"].replace("Z", "+00:00")).astimezone(tz).strftime("%Y-%m-%d %H:%M")} for e in events]
await submit_answer({"events": out})
