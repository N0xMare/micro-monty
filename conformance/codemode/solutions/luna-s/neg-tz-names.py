from datetime import datetime
from zoneinfo import ZoneInfo

events = await list_events({"start": "2026-03-09", "end": "2026-03-11", "attendee": "diego"})
zone = ZoneInfo("America/New_York")
selected = [e for e in events if e["start"][:10] in {"2026-03-09", "2026-03-10"}]
selected.sort(key=lambda e: datetime.fromisoformat(e["start"].replace("Z", "+00:00")))
answer = [{
    "id": e["id"],
    "start_ny": datetime.fromisoformat(e["start"].replace("Z", "+00:00")).astimezone(zone).strftime("%Y-%m-%d %H:%M"),
} for e in selected]
await submit_answer({"events": answer})
