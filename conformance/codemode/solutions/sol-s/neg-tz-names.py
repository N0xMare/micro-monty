from datetime import datetime
from zoneinfo import ZoneInfo

events = await list_events({"start": "2026-03-08T00:00:00Z", "end": "2026-03-12T00:00:00Z", "attendee": "diego"})
selected = [event for event in events if event["start"][:10] in ("2026-03-09", "2026-03-10")]
selected.sort(key=lambda event: datetime.fromisoformat(event["start"]))
await submit_answer({"events": [{"id": event["id"], "start_ny": datetime.fromisoformat(event["start"]).astimezone(ZoneInfo("America/New_York")).strftime("%Y-%m-%d %H:%M")} for event in selected]})
