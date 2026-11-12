import json
from datetime import datetime
from itertools import combinations

events = json.loads(await list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-16T00:00:00Z", "attendee": "alice"}))
pairs = []
for a, b in combinations(events, 2):
    if max(datetime.fromisoformat(a["start"]), datetime.fromisoformat(b["start"])) < min(datetime.fromisoformat(a["end"]), datetime.fromisoformat(b["end"])):
        pairs.append(sorted([a["id"], b["id"]]))
await submit_answer({"pairs": sorted(pairs)})
