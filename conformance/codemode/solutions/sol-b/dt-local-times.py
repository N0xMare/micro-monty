import asyncio, json
from datetime import datetime
from zoneinfo import ZoneInfo
raw_events, raw_profile = await asyncio.gather(list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T00:00:00Z", "attendee": "chitra"}), get_user_profile({"user": "chitra"}))
zone = ZoneInfo(json.loads(raw_profile)["timezone"])
events = sorted(json.loads(raw_events), key=lambda e: datetime.fromisoformat(e["start"]))
await submit_answer({"events": [{"id": e["id"], "local_start": datetime.fromisoformat(e["start"]).astimezone(zone).strftime("%Y-%m-%d %H:%M")} for e in events]})
