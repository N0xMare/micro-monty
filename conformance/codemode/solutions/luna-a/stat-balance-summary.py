import json
import math

accounts = []
offset = 0
while True:
    page = json.loads(await list_accounts({"offset": offset, "limit": 10}))
    batch = page["accounts"]
    accounts.extend(batch)
    offset += len(batch)
    if not batch or offset >= page["total"]:
        break
active = [a for a in accounts if a["status"] == "active"]
balances = [float(a["balance"]) for a in active]
positive = [x for x in balances if x > 0]
geo = math.exp(sum(math.log(x) for x in positive) / len(positive)) if positive else 0.0
harm = len(positive) / sum(1 / x for x in positive) if positive else 0.0
await submit_answer({"count": len(active), "sum": round(sum(balances), 2), "geometric_mean": round(geo, 2), "harmonic_mean": round(harm, 2)})
