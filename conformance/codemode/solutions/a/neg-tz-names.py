import json
from datetime import datetime, timedelta, timezone

try:
    from zoneinfo import ZoneInfo
    NY = ZoneInfo("America/New_York")
    def to_ny(dt):
        return dt.astimezone(NY)
except Exception:
    # Fallback without tzdata: 2026 DST runs 2026-03-08 07:00 UTC to 2026-11-01 06:00 UTC.
    DST_START = datetime(2026, 3, 8, 7, 0, tzinfo=timezone.utc)
    DST_END = datetime(2026, 11, 1, 6, 0, tzinfo=timezone.utc)
    def to_ny(dt):
        off = -4 if DST_START <= dt < DST_END else -5
        return dt.astimezone(timezone(timedelta(hours=off)))

# Query a padded window so events written with any offset are included, then filter on the written date.
events = json.loads(await list_events({"start": "2026-03-08", "end": "2026-03-12", "attendee": "diego"}))
picked = []
for e in events:
    if e["start"][:10] in ("2026-03-09", "2026-03-10") and "diego" in e.get("attendees", ["diego"]):
        dt = datetime.fromisoformat(e["start"].replace("Z", "+00:00"))
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        picked.append((dt, e["id"], e["start"]))
picked.sort(key=lambda p: p[0])
out = [{"id": i, "start_ny": to_ny(dt).strftime("%Y-%m-%d %H:%M")} for dt, i, _ in picked]
for (dt, i, s), o in zip(picked, out):
    print(i, s, "->", o["start_ny"])
print(await submit_answer({"events": out}))
