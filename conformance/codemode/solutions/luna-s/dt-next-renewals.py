import calendar
import datetime

subscriptions = await list_subscriptions({})
as_of = datetime.date(2026, 3, 15)
renewals = []
for subscription in subscriptions:
    if subscription["cancelled_at"] is not None:
        continue
    first = datetime.date.fromisoformat(subscription["started"]) + datetime.timedelta(days=int(subscription["trial_days"]))
    period = {"monthly": 1, "quarterly": 3, "annual": 12}[subscription["plan"]]
    months = (as_of.year - first.year) * 12 + as_of.month - first.month
    k = max(1, months // period)
    while True:
        target_month = first.month - 1 + k * period
        year = first.year + target_month // 12
        month = target_month % 12 + 1
        date = datetime.date(year, month, min(first.day, calendar.monthrange(year, month)[1]))
        if date > as_of:
            break
        k += 1
    renewals.append({"id": subscription["id"], "next_renewal": date.isoformat()})
await submit_answer({"renewals": renewals})
