import json, asyncio
from datetime import datetime
from zoneinfo import ZoneInfo

events_raw, profile_raw = await asyncio.gather(
    list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T00:00:00Z", "attendee": "chitra"}),
    get_user_profile({"user": "chitra"}),
)
events = json.loads(events_raw)
tz = ZoneInfo(json.loads(profile_raw)["timezone"])

out = []
for e in sorted(events, key=lambda e: datetime.fromisoformat(e["start"])):
    if "chitra" not in e.get("attendees", []):
        continue
    local = datetime.fromisoformat(e["start"]).astimezone(tz)
    out.append({"id": e["id"], "local_start": local.strftime("%Y-%m-%d %H:%M")})

print(tz, out)
print(await submit_answer({"events": out}))
