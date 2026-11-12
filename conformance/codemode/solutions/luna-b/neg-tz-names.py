import json
from datetime import datetime
from zoneinfo import ZoneInfo

events = json.loads(await list_events({"start": "2026-03-09", "end": "2026-03-11", "attendee": "diego"}))
selected = []
for event in events:
    if event["start"][:10] in {"2026-03-09", "2026-03-10"}:
        local = datetime.fromisoformat(event["start"].replace("Z", "+00:00")).astimezone(ZoneInfo("America/New_York"))
        selected.append({"id": event["id"], "start_ny": local.strftime("%Y-%m-%d %H:%M")})
await submit_answer({"events": selected})
