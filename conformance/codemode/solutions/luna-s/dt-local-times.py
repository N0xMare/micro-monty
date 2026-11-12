import datetime
from zoneinfo import ZoneInfo

profile, events = await asyncio.gather(get_user_profile({"user": "chitra"}), list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T00:00:00Z", "attendee": "chitra"}))
zone = ZoneInfo(profile["timezone"])
local_events = [{"id": event["id"], "local_start": datetime.datetime.fromisoformat(event["start"].replace("Z", "+00:00")).astimezone(zone).strftime("%Y-%m-%d %H:%M")} for event in events]
await submit_answer({"events": local_events})
