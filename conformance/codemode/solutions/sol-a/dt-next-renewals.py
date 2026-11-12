import calendar
import json
from datetime import date, timedelta

cutoff = date(2026, 3, 15)
renewals = []
for subscription in json.loads(await list_subscriptions({})):
    if subscription["cancelled_at"] is not None:
        continue
    first = date.fromisoformat(subscription["started"]) + timedelta(days=subscription["trial_days"])
    period = {"monthly": 1, "quarterly": 3, "annual": 12}[subscription["plan"]]
    k = 0
    while True:
        month_index = first.year * 12 + first.month - 1 + k * period
        year, month_zero = divmod(month_index, 12)
        month = month_zero + 1
        renewal = date(year, month, min(first.day, calendar.monthrange(year, month)[1]))
        if renewal > cutoff:
            renewals.append({"id": subscription["id"], "next_renewal": renewal.isoformat()})
            break
        k += 1
await submit_answer({"renewals": renewals})
