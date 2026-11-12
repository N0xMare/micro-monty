from datetime import datetime, timezone, time
from zoneinfo import ZoneInfo
import asyncio

def parse(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt

def hm(s):
    h, m = map(int, s.split(":"))
    return h * 60 + m

profile, events = await asyncio.gather(
    get_user_profile({"user": "emi"}),
    list_events({"start": "2026-03-09T00:00:00Z", "end": "2026-03-23T00:00:00Z", "attendee": "emi"}),
)
tz = ZoneInfo(profile["timezone"])
ws, we = hm(profile["working_hours"]["start"]), hm(profile["working_hours"]["end"])
lo = datetime(2026, 3, 9, tzinfo=timezone.utc)
hi = datetime(2026, 3, 23, tzinfo=timezone.utc)
rows = []
for e in events:
    if "emi" not in (e.get("attendees") or []):
        continue
    st = parse(e["start"])
    if not (lo <= st < hi):
        continue
    loc = st.astimezone(tz)
    mins = loc.hour * 60 + loc.minute + loc.second / 60
    if ws <= we:
        inside = ws <= mins < we
    else:  # overnight shift
        inside = mins >= ws or mins < we
    if not inside:
        rows.append((st, e["id"]))
rows.sort(key=lambda r: r[0])
ids = [i for _, i in rows]
print(ids)
await submit_answer({"event_ids": ids})
