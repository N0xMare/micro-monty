import json
import math
accounts = []
offset = 0
while True:
    page = json.loads(await list_accounts({"offset": offset, "limit": 10}))
    accounts.extend(page["accounts"])
    offset += len(page["accounts"])
    if offset >= page["total"]:
        break
balances = [a["balance"] for a in accounts if a["status"] == "active"]
positive = [b for b in balances if b > 0]
await submit_answer({"count": len(balances), "sum": round(math.fsum(balances), 2), "geometric_mean": round(math.exp(math.fsum(map(math.log, positive)) / len(positive)), 2), "harmonic_mean": round(len(positive) / math.fsum(1 / b for b in positive), 2)})
