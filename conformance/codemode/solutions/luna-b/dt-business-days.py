import json
from datetime import date, timedelta

holidays = set(json.loads(await list_holidays({"country": "US", "year": 2026})))
start, end = date(2026, 3, 15), date(2026, 7, 15)
count = sum(1 for i in range((end - start).days + 1) if (d := start + timedelta(days=i)).weekday() < 5 and d.isoformat() not in holidays)
await submit_answer({"business_days": count})
