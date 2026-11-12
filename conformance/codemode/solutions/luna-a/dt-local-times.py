import json
from datetime import datetime
from zoneinfo import ZoneInfo
p = json.loads(await get_user_profile({"user": "chitra"}))
events = json.loads(await list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T00:00:00Z", "attendee": "chitra"}))
zone = ZoneInfo(p["timezone"])
out = [{"id": e["id"], "local_start": datetime.fromisoformat(e["start"].replace("Z", "+00:00")).astimezone(zone).strftime("%Y-%m-%d %H:%M")} for e in events]
await submit_answer({"events": out})
