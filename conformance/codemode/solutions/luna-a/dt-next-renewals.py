import json
import calendar
from datetime import date, timedelta
subs = json.loads(await list_subscriptions({})); as_of = date(2026,3,15)
periods = {"monthly": 1, "quarterly": 3, "annual": 12}; output = []
for sub in subs:
    if sub["cancelled_at"] is not None: continue
    first = date.fromisoformat(sub["started"]) + timedelta(days=sub["trial_days"])
    k = 0
    while True:
        idx = first.year*12 + first.month-1 + k*periods[sub["plan"]]
        year, m0 = divmod(idx, 12); month = m0+1
        renew = date(year, month, min(first.day, calendar.monthrange(year, month)[1]))
        if renew > as_of: break
        k += 1
    output.append({"id": sub["id"], "next_renewal": renew.isoformat()})
await submit_answer({"renewals": output})
