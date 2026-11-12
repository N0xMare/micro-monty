import calendar, json
from datetime import date, timedelta

subs = json.loads(await list_subscriptions({}))
target = date(2026, 3, 15)
renewals = []
months_per = {"monthly": 1, "quarterly": 3, "annual": 12}
for sub in subs:
    if sub["cancelled_at"] is not None:
        continue
    first = date.fromisoformat(sub["started"]) + timedelta(days=sub["trial_days"])
    period = months_per[sub["plan"]]
    k = 1
    while True:
        total_month = first.year * 12 + first.month - 1 + period * k
        year, month = divmod(total_month, 12)
        month += 1
        renewal = date(year, month, min(first.day, calendar.monthrange(year, month)[1]))
        if renewal > target:
            break
        k += 1
    renewals.append({"id": sub["id"], "next_renewal": renewal.isoformat()})
await submit_answer({"renewals": renewals})
