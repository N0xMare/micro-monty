import json
from datetime import datetime
from zoneinfo import ZoneInfo
p = json.loads(await get_user_profile({"user": "emi"}))
events = json.loads(await list_events({"start": "2026-03-09T00:00Z", "end": "2026-03-23T00:00Z", "attendee": "emi"}))
zone = ZoneInfo(p["timezone"]); start = p["working_hours"]["start"]; end = p["working_hours"]["end"]
ids = []
for e in events:
    local = datetime.fromisoformat(e["start"].replace("Z", "+00:00")).astimezone(zone)
    clock = local.strftime("%H:%M")
    if clock < start or clock >= end: ids.append(e["id"])
await submit_answer({"event_ids": ids})
