import json
from datetime import datetime

events = json.loads(await list_events({"start": "2026-03-16T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "alice"}))
seconds = sum((datetime.fromisoformat(e["end"].replace("Z", "+00:00")) - datetime.fromisoformat(e["start"].replace("Z", "+00:00"))).total_seconds() for e in events)
await submit_answer({"hours": round(seconds / 3600, 2), "events": len(events)})
