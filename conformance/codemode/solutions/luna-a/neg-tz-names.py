import json
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

events = json.loads(await list_events({"start": "2026-03-09", "end": "2026-03-11", "attendee": "diego"}))
ny = ZoneInfo("America/New_York")
result = []
for event in events:
    if event["start"][:10] not in {"2026-03-09", "2026-03-10"}:
        continue
    start = datetime.fromisoformat(event["start"].replace("Z", "+00:00"))
    if start.tzinfo is None:
        start = start.replace(tzinfo=timezone.utc)
    result.append({"id": event["id"], "start_ny": start.astimezone(ny).strftime("%Y-%m-%d %H:%M")})
result.sort(key=lambda item: next(event["start"] for event in events if event["id"] == item["id"]))
await submit_answer({"events": result})
