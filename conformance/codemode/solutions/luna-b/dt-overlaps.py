import json
from datetime import datetime

events = json.loads(await list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-16T00:00:00Z", "attendee": "alice"}))
intervals = [(e["id"], datetime.fromisoformat(e["start"].replace("Z", "+00:00")), datetime.fromisoformat(e["end"].replace("Z", "+00:00"))) for e in events]
pairs = []
for i, (id1, start1, end1) in enumerate(intervals):
    for id2, start2, end2 in intervals[i + 1:]:
        if start1 < end2 and start2 < end1:
            pairs.append(sorted((id1, id2)))
await submit_answer({"pairs": sorted(pairs)})
