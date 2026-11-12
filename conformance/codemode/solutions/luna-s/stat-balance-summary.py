import math

accounts = []
offset = 0
while True:
    page = await list_accounts({"offset": offset, "limit": 10})
    accounts.extend(page["accounts"])
    offset += len(page["accounts"])
    if offset >= page["total"] or not page["accounts"]:
        break
active = [a["balance"] for a in accounts if a["status"] == "active"]
positive = [x for x in active if x > 0]
count = len(active)
total = round(sum(active), 2)
geometric = round(math.exp(sum(math.log(x) for x in positive) / len(positive)), 2) if positive else 0.0
harmonic = round(len(positive) / sum(1 / x for x in positive), 2) if positive else 0.0
await submit_answer({"count": count, "sum": total, "geometric_mean": geometric, "harmonic_mean": harmonic})
