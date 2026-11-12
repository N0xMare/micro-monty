import json
from datetime import datetime

events = json.loads(await list_events({"start": "2026-03-16T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "alice"}))
events = [e for e in events if "alice" in e.get("attendees", [])]

total = sum(
    (datetime.fromisoformat(e["end"]) - datetime.fromisoformat(e["start"])).total_seconds()
    for e in events
)
answer = {"hours": round(total / 3600, 2), "events": len(events)}
print(answer)
print(await submit_answer(answer))
