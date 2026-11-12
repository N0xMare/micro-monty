import asyncio, json
from datetime import datetime, time
from zoneinfo import ZoneInfo
raw_events, raw_profile = await asyncio.gather(list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "emi"}), get_user_profile({"user": "emi"}))
profile = json.loads(raw_profile)
zone = ZoneInfo(profile["timezone"])
lo, hi = (time.fromisoformat(profile["working_hours"][k]) for k in ("start", "end"))
events = sorted(json.loads(raw_events), key=lambda e: datetime.fromisoformat(e["start"]))
await submit_answer({"event_ids": [e["id"] for e in events if not lo <= datetime.fromisoformat(e["start"]).astimezone(zone).time() < hi]})
