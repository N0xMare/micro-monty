import json
from datetime import datetime, timedelta, timezone
attendees = ["alice", "bjorn", "diego"]
events = json.loads(await list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-19T00:00:00Z"}))
candidate, latest = datetime(2026,3,18,13,tzinfo=timezone.utc), datetime(2026,3,18,16,tzinfo=timezone.utc)
while candidate <= latest:
    finish = candidate + timedelta(minutes=60)
    busy = False
    for event in events:
        if not set(event["attendees"]).intersection(attendees): continue
        es = datetime.fromisoformat(event["start"].replace("Z", "+00:00"))
        ee = datetime.fromisoformat(event["end"].replace("Z", "+00:00"))
        if es < finish and ee > candidate: busy = True; break
    if not busy:
        await create_event({"title": "Sync", "start": candidate.isoformat(), "end": finish.isoformat(), "attendees": attendees})
        break
    candidate += timedelta(minutes=15)
