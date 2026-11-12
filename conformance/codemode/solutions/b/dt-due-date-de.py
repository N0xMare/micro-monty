import asyncio, json
from datetime import date, timedelta

h26, h27 = await asyncio.gather(
    list_holidays({"country": "DE", "year": 2026}),
    list_holidays({"country": "DE", "year": 2027}),
)
holidays = {date.fromisoformat(d) for d in json.loads(h26) + json.loads(h27)}

d = date(2026, 3, 15)
remaining = 30
skipped = []
while remaining:
    d += timedelta(days=1)
    if d.weekday() < 5 and d not in holidays:
        remaining -= 1
    elif d in holidays:
        skipped.append(d.isoformat())

print("holidays skipped:", skipped, "due:", d)
print(await submit_answer({"due_date": d.isoformat()}))
