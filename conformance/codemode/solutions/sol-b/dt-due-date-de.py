import json
from datetime import date, timedelta
holidays = set(json.loads(await list_holidays({"country": "DE", "year": 2026})))
day, count = date(2026, 3, 15), 0
while count < 30:
    day += timedelta(days=1)
    if day.weekday() < 5 and day.isoformat() not in holidays:
        count += 1
await submit_answer({"due_date": day.isoformat()})
