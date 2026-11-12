import json
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

profile, events = await asyncio.gather(
    get_user_profile({"user": "chitra"}),
    list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T00:00:00Z", "attendee": "chitra"}),
) if (asyncio := __import__("asyncio")) else None
profile = json.loads(profile)
events = json.loads(events)
tz = ZoneInfo(profile["timezone"])

def parse(s):
    d = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if d.tzinfo is None:
        d = d.replace(tzinfo=timezone.utc)
    return d

lo = datetime(2026, 3, 17, tzinfo=timezone.utc)
hi = datetime(2026, 3, 18, tzinfo=timezone.utc)
rows = []
for e in events:
    if "chitra" not in e.get("attendees", []):
        continue
    s = parse(e["start"])
    if not (lo <= s < hi):
        continue
    rows.append((s, e["id"]))
rows.sort(key=lambda r: r[0])
result = [{"id": i, "local_start": s.astimezone(tz).strftime("%Y-%m-%d %H:%M")} for s, i in rows]
print(result)
print(await submit_answer({"events": result}))
