import json, calendar
from datetime import date, timedelta

subs = json.loads(await list_subscriptions({}))
cutoff = date(2026, 3, 15)
months_per = {"monthly": 1, "quarterly": 3, "annual": 12}

def add_months(d, n):
    m = d.month - 1 + n
    y, m = d.year + m // 12, m % 12 + 1
    return date(y, m, min(d.day, calendar.monthrange(y, m)[1]))

out = []
for s in subs:
    if s.get("cancelled_at"):
        continue
    first = date.fromisoformat(s["started"]) + timedelta(days=s.get("trial_days") or 0)
    step = months_per[s["plan"]]
    k = 0
    nxt = first
    while nxt <= cutoff:
        k += 1
        nxt = add_months(first, k * step)
    out.append({"id": s["id"], "next_renewal": nxt.isoformat()})

print(out)
print(await submit_answer({"renewals": out}))
