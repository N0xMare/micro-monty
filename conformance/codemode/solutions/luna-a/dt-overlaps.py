import json
from datetime import datetime
events = json.loads(await list_events({"start": "2026-03-09T00:00Z", "end": "2026-03-16T00:00Z", "attendee": "alice"}))
pairs = set()
for i, a in enumerate(events):
    a0 = datetime.fromisoformat(a["start"].replace("Z", "+00:00")); a1 = datetime.fromisoformat(a["end"].replace("Z", "+00:00"))
    for b in events[i+1:]:
        b0 = datetime.fromisoformat(b["start"].replace("Z", "+00:00")); b1 = datetime.fromisoformat(b["end"].replace("Z", "+00:00"))
        if a0 < b1 and b0 < a1: pairs.add(tuple(sorted((a["id"], b["id"]))))
await submit_answer({"pairs": [list(p) for p in sorted(pairs)]})
