from datetime import date, timedelta
from calendar import monthrange

subscriptions = await list_subscriptions({})
cutoff = date(2026, 3, 15)
periods = {"monthly": 1, "quarterly": 3, "annual": 12}
renewals = []
for subscription in subscriptions:
    if subscription["cancelled_at"] is not None:
        continue
    first = date.fromisoformat(subscription["started"]) + timedelta(days=subscription["trial_days"])
    period = periods[subscription["plan"]]
    k = 0
    while True:
        month_index = first.year * 12 + first.month - 1 + k * period
        year, month0 = divmod(month_index, 12)
        month = month0 + 1
        renewal = date(year, month, min(first.day, monthrange(year, month)[1]))
        if renewal > cutoff:
            break
        k += 1
    renewals.append({"id": subscription["id"], "next_renewal": renewal.isoformat()})
await submit_answer({"renewals": renewals})
