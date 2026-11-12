from datetime import date, timedelta

holidays = set(await list_holidays({"country": "DE", "year": 2026}))
day = date(2026, 3, 15)
remaining = 30
while remaining:
    day += timedelta(days=1)
    if day.weekday() < 5 and day.isoformat() not in holidays:
        remaining -= 1
await submit_answer({"due_date": day.isoformat()})
