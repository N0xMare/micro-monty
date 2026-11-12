from datetime import datetime
from itertools import combinations

events = await list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-16T00:00:00Z", "attendee": "alice"})
intervals = [(e["id"], datetime.fromisoformat(e["start"]), datetime.fromisoformat(e["end"])) for e in events]
pairs = [sorted([a[0], b[0]]) for a, b in combinations(intervals, 2) if max(a[1], b[1]) < min(a[2], b[2])]
await submit_answer({"pairs": sorted(pairs)})
