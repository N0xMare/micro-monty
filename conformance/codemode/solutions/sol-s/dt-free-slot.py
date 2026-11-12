import asyncio
from datetime import datetime, timedelta, timezone

attendees = ["alice", "bjorn", "diego"]
calendars = await asyncio.gather(*(list_events({"start": "0001-01-01T00:00:00Z", "end": "2026-03-18T17:00:00Z", "attendee": user}) for user in attendees))
busy = [(datetime.fromisoformat(e["start"]), datetime.fromisoformat(e["end"])) for events in calendars for e in events]
start = datetime(2026, 3, 18, 13, tzinfo=timezone.utc)
limit = datetime(2026, 3, 18, 17, tzinfo=timezone.utc)
while start + timedelta(hours=1) <= limit:
    end = start + timedelta(hours=1)
    if all(not (a < end and b > start) for a, b in busy):
        result = await create_event({"title": "Sync", "start": start.isoformat(), "end": end.isoformat(), "attendees": attendees})
        print(result)
        break
    start += timedelta(minutes=15)
else:
    print("No available 60-minute slot in the requested window.")
