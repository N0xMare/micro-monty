from datetime import datetime, timedelta, timezone

try:
    from zoneinfo import ZoneInfo
    NY = ZoneInfo("America/New_York")
except Exception:
    # DST started 2026-03-08 and ends in November, so these dates are EDT (UTC-4).
    NY = timezone(timedelta(hours=-4))

# Query a wider window: an event's written date can differ from its UTC date.
events = await list_events({"start": "2026-03-08T00:00:00Z", "end": "2026-03-12T00:00:00Z",
                            "attendee": "diego"})
picked = []
for e in events:
    if "diego" not in e.get("attendees", []):
        continue
    if e["start"][:10] not in ("2026-03-09", "2026-03-10"):
        continue
    dt = datetime.fromisoformat(e["start"].replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    picked.append((dt, {"id": e["id"], "start_ny": dt.astimezone(NY).strftime("%Y-%m-%d %H:%M")}))
picked.sort(key=lambda p: p[0])
result = [p[1] for p in picked]
for r in result:
    print(r)
print(await submit_answer({"events": result}))
