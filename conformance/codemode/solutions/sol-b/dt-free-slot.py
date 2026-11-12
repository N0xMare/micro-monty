import asyncio, json
from datetime import datetime, timedelta, timezone
attendees = ["alice", "bjorn", "diego"]
raw = await asyncio.gather(*(list_events({"start": "0001-01-01T00:00:00Z", "end": "2026-03-18T17:00:00Z", "attendee": user}) for user in attendees))
busy = [(datetime.fromisoformat(e["start"]), datetime.fromisoformat(e["end"])) for result in raw for e in json.loads(result)]
start = datetime(2026, 3, 18, 13, tzinfo=timezone.utc)
limit = datetime(2026, 3, 18, 17, tzinfo=timezone.utc)
while start + timedelta(hours=1) <= limit:
    end = start + timedelta(hours=1)
    if not any(a < end and start < b for a, b in busy):
        print(await create_event({"title": "Sync", "start": start.isoformat(), "end": end.isoformat(), "attendees": attendees}))
        break
    start += timedelta(minutes=15)
else:
    raise ValueError("No available 60-minute slot")
