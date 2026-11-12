import json, calendar
from datetime import date, timedelta
subscriptions = json.loads(await list_subscriptions({}))
cutoff = date(2026, 3, 15)
renewals = []
for s in subscriptions:
    if s["cancelled_at"] is not None:
        continue
    first = date.fromisoformat(s["started"]) + timedelta(days=s["trial_days"])
    period = {"monthly": 1, "quarterly": 3, "annual": 12}[s["plan"]]
    k = 0
    while True:
        year, month0 = divmod(first.year * 12 + first.month - 1 + k * period, 12)
        renewal = date(year, month0 + 1, min(first.day, calendar.monthrange(year, month0 + 1)[1]))
        if renewal > cutoff:
            renewals.append({"id": s["id"], "next_renewal": renewal.isoformat()})
            break
        k += 1
await submit_answer({"renewals": renewals})
