import datetime
from zoneinfo import ZoneInfo

profile, events = await asyncio.gather(get_user_profile({"user": "emi"}), list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "emi"}))
zone = ZoneInfo(profile["timezone"])
start_hour, start_minute = map(int, profile["working_hours"]["start"].split(":"))
end_hour, end_minute = map(int, profile["working_hours"]["end"].split(":"))
work_start = start_hour * 60 + start_minute
work_end = end_hour * 60 + end_minute
outside = []
for event in events:
    local = datetime.datetime.fromisoformat(event["start"].replace("Z", "+00:00")).astimezone(zone)
    local_minute = local.hour * 60 + local.minute
    if local_minute < work_start or local_minute >= work_end:
        outside.append(event["id"])
await submit_answer({"event_ids": outside})
