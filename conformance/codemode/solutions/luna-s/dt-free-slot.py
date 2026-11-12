import datetime

attendees = ["alice", "bjorn", "diego"]
events_by_person = await asyncio.gather(*(list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-19T00:00:00Z", "attendee": person}) for person in attendees))
events = [event for person_events in events_by_person for event in person_events]
day = datetime.date(2026, 3, 18)
chosen = None
for minute in range(13 * 60, 16 * 60 + 1, 15):
    start = datetime.datetime.combine(day, datetime.time(minute // 60, minute % 60), tzinfo=datetime.timezone.utc)
    end = start + datetime.timedelta(minutes=60)
    if all(not (datetime.datetime.fromisoformat(event["start"].replace("Z", "+00:00")) < end and datetime.datetime.fromisoformat(event["end"].replace("Z", "+00:00")) > start) for event in events):
        chosen = (start, end)
        break
if chosen is not None:
    await create_event({"title": "Sync", "start": chosen[0].isoformat(), "end": chosen[1].isoformat(), "attendees": attendees})
