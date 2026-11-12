import asyncio
from datetime import date, timedelta

h26, h27 = await asyncio.gather(
    list_holidays({"country": "DE", "year": 2026}),
    list_holidays({"country": "DE", "year": 2027}),
)
holidays = set(h26) | set(h27)

d = date(2026, 3, 15)
counted = 0
while counted < 30:
    d += timedelta(days=1)
    if d.weekday() < 5 and d.isoformat() not in holidays:
        counted += 1
print(d.isoformat(), sorted(h for h in holidays if "2026-03-15" < h <= d.isoformat()))
print(await submit_answer({"due_date": d.isoformat()}))
