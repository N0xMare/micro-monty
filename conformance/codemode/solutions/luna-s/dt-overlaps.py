import datetime
import itertools

events = await list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-16T00:00:00Z", "attendee": "alice"})
pairs = []
for first, second in itertools.combinations(events, 2):
    first_start = datetime.datetime.fromisoformat(first["start"].replace("Z", "+00:00"))
    first_end = datetime.datetime.fromisoformat(first["end"].replace("Z", "+00:00"))
    second_start = datetime.datetime.fromisoformat(second["start"].replace("Z", "+00:00"))
    second_end = datetime.datetime.fromisoformat(second["end"].replace("Z", "+00:00"))
    if first_start < second_end and second_start < first_end:
        pairs.append(sorted((first["id"], second["id"])))
await submit_answer({"pairs": sorted(pairs)})
