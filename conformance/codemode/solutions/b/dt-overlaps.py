import json
from datetime import datetime

events = json.loads(await list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-16T00:00:00Z", "attendee": "alice"}))
events = [e for e in events if "alice" in e.get("attendees", [])]
spans = [(e["id"], datetime.fromisoformat(e["start"]), datetime.fromisoformat(e["end"])) for e in events]

pairs = []
for i in range(len(spans)):
    for j in range(i + 1, len(spans)):
        a, b = spans[i], spans[j]
        if a[1] < b[2] and b[1] < a[2]:
            pairs.append(sorted([a[0], b[0]]))
pairs.sort()

print(len(events), pairs)
print(await submit_answer({"pairs": pairs}))
