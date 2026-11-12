from datetime import date, timedelta
import calendar

def add_months(d, n):
    m = d.month - 1 + n
    y = d.year + m // 12
    m = m % 12 + 1
    return date(y, m, min(d.day, calendar.monthrange(y, m)[1]))

period = {"monthly": 1, "quarterly": 3, "annual": 12}
cutoff = date(2026, 3, 15)
subs = await list_subscriptions({})
out = []
for s in subs:
    if s.get("cancelled_at"):
        continue
    first = date.fromisoformat(s["started"]) + timedelta(days=s.get("trial_days") or 0)
    p = period[s["plan"]]
    k = 1
    while True:
        r = add_months(first, k * p)
        if r > cutoff:
            break
        k += 1
    out.append({"id": s["id"], "next_renewal": r.isoformat()})
print(out)
await submit_answer({"renewals": out})
