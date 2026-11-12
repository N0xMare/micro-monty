from datetime import date, timedelta

holidays = set(await list_holidays({"country": "US", "year": 2026}))
day = date(2026, 3, 15)
end = date(2026, 7, 15)
count = 0
while day <= end:
    count += day.weekday() < 5 and day.isoformat() not in holidays
    day += timedelta(days=1)
await submit_answer({"business_days": count})
