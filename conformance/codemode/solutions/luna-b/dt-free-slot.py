import json
from datetime import datetime, timedelta, timezone

events = json.loads(await list_events({"start": "2026-03-18T00:00:00Z", "end": "2026-03-19T00:00:00Z"}))
people = ("alice", "bjorn", "diego")
for minutes in range(13 * 60, 16 * 60 + 1, 15):
    start = datetime(2026, 3, 18, minutes // 60, minutes % 60, tzinfo=timezone.utc)
    end = start + timedelta(minutes=60)
    if all(not (person in e["attendees"] and datetime.fromisoformat(e["start"].replace("Z", "+00:00")) < end and datetime.fromisoformat(e["end"].replace("Z", "+00:00")) > start) for person in people for e in events):
        await create_event({"title": "Sync", "start": start.isoformat().replace("+00:00", "Z"), "end": end.isoformat().replace("+00:00", "Z"), "attendees": ["alice", "bjorn", "diego"]})
        break
