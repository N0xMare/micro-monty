import json
from datetime import datetime, timedelta, timezone

# Query a wider UTC window so events written with any offset on 03-09/03-10 are included.
events = json.loads(await list_events({"start": "2026-03-08", "end": "2026-03-12", "attendee": "diego"}))
ny = timezone(timedelta(hours=-4))  # EDT: DST began 2026-03-08, so 03-09/03-10 are UTC-4
out = []
for e in events:
    if "diego" not in e.get("attendees", []):
        continue
    if e["start"][:10] not in ("2026-03-09", "2026-03-10"):
        continue
    dt = datetime.fromisoformat(e["start"].replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    out.append((dt, {"id": e["id"], "start_ny": dt.astimezone(ny).strftime("%Y-%m-%d %H:%M")}))
out.sort(key=lambda p: p[0])
result = [o for _, o in out]
print(result)
print(await submit_answer({"events": result}))
