from statistics import geometric_mean, harmonic_mean
from math import fsum

accounts = []
offset = 0
while True:
    page = await list_accounts({"offset": offset, "limit": 10})
    accounts.extend(page["accounts"])
    offset += len(page["accounts"])
    if offset >= page["total"]:
        break
balances = [a["balance"] for a in accounts if a["status"] == "active"]
positive = [b for b in balances if b > 0]
await submit_answer({"count": len(balances), "sum": round(fsum(balances), 2), "geometric_mean": round(geometric_mean(positive), 2), "harmonic_mean": round(harmonic_mean(positive), 2)})
