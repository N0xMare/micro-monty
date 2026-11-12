from datetime import date, timedelta

holidays = set(await list_holidays({"country": "US", "year": 2026}))
start, end = date(2026, 3, 15), date(2026, 7, 15)
count = 0
d = start
while d <= end:
    if d.weekday() < 5 and d.isoformat() not in holidays:
        count += 1
    d += timedelta(days=1)
print(count, sorted(h for h in holidays if start.isoformat() <= h <= end.isoformat()))
print(await submit_answer({"business_days": count}))
