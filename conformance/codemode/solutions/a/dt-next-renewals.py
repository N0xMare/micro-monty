import json
import calendar
from datetime import date, timedelta

subs = json.loads(await list_subscriptions({}))
months = {"monthly": 1, "quarterly": 3, "annual": 12}
ref = date(2026, 3, 15)

def add_months(d, n):
    m = d.month - 1 + n
    y = d.year + m // 12
    m = m % 12 + 1
    return date(y, m, min(d.day, calendar.monthrange(y, m)[1]))

out = []
for s in subs:
    if s.get("cancelled_at"):
        continue
    first = date.fromisoformat(s["started"]) + timedelta(days=s.get("trial_days") or 0)
    step = months[s["plan"]]
    k = 1
    while (r := add_months(first, k * step)) <= ref:
        k += 1
    out.append({"id": s["id"], "next_renewal": r.isoformat()})
print(out)
print(await submit_answer({"renewals": out}))
