import json
import math

accounts, offset, total = [], 0, None
while total is None or offset < total:
    page = json.loads(await list_accounts({"offset": offset, "limit": 10}))
    accounts.extend(page["accounts"]); total = page["total"]
    offset += len(page["accounts"])
active = [a["balance"] for a in accounts if a["status"] == "active"]
positive = [x for x in active if x > 0]
await submit_answer({"count": len(active), "sum": round(sum(active), 2), "geometric_mean": round(math.exp(sum(math.log(x) for x in positive) / len(positive)), 2), "harmonic_mean": round(len(positive) / sum(1 / x for x in positive), 2)})
