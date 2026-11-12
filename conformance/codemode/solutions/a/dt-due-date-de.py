import asyncio, json
from datetime import date, timedelta

hol_lists = await asyncio.gather(*[list_holidays({"country": "DE", "year": y}) for y in (2026, 2027)])
holidays = {date.fromisoformat(d) for s in hol_lists for d in json.loads(s)}

d = date(2026, 3, 15)
counted = 0
while counted < 30:
    d += timedelta(days=1)
    if d.weekday() < 5 and d not in holidays:
        counted += 1
print(d.isoformat(), sorted(h.isoformat() for h in holidays if date(2026, 3, 15) < h <= d))
print(await submit_answer({"due_date": d.isoformat()}))
