from datetime import datetime, timezone
from zoneinfo import ZoneInfo
import asyncio

profile, events = await asyncio.gather(
    get_user_profile({"user": "chitra"}),
    list_events({"start": "2026-03-17T00:00:00Z", "end": "2026-03-18T00:00:00Z", "attendee": "chitra"}),
)
tz = ZoneInfo(profile["timezone"])

def parse(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt

day_start = datetime(2026, 3, 17, tzinfo=timezone.utc)
day_end = datetime(2026, 3, 18, tzinfo=timezone.utc)
rows = []
for e in events:
    if "chitra" not in (e.get("attendees") or []):
        continue
    st = parse(e["start"])
    if not (day_start <= st < day_end):
        continue
    rows.append((st, e["id"], st.astimezone(tz).strftime("%Y-%m-%d %H:%M")))
rows.sort(key=lambda r: r[0])
result = [{"id": i, "local_start": ls} for _, i, ls in rows]
print(result)
await submit_answer({"events": result})
