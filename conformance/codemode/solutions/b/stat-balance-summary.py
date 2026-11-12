import json
import math

accounts = []
offset = 0
while True:
    page = json.loads(await list_accounts({"offset": offset, "limit": 10}))
    batch = page["accounts"]
    accounts += batch
    offset += len(batch)
    if not batch or offset >= page["total"]:
        break
print(len(accounts), "accounts fetched")

active = [a["balance"] for a in accounts if a["status"] == "active"]
positive = [b for b in active if b > 0]
geo = math.exp(sum(math.log(b) for b in positive) / len(positive))
harm = len(positive) / sum(1 / b for b in positive)

answer = {
    "count": len(active),
    "sum": round(sum(active), 2),
    "geometric_mean": round(geo, 2),
    "harmonic_mean": round(harm, 2),
}
print(answer)
print(await submit_answer(answer))
