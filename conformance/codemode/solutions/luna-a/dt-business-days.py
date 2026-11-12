import json
from datetime import date, timedelta
holidays = set(json.loads(await list_holidays({"country": "US", "year": 2026})))
d, end, count = date(2026, 3, 15), date(2026, 7, 15), 0
while d <= end:
    if d.weekday() < 5 and d.isoformat() not in holidays: count += 1
    d += timedelta(days=1)
await submit_answer({"business_days": count})
