import asyncio
import json
import math

first = json.loads(await list_accounts({"offset": 0, "limit": 10}))
total = first["total"]
pages = [first] + [
    json.loads(r)
    for r in await asyncio.gather(
        *(list_accounts({"offset": off, "limit": 10}) for off in range(10, total, 10))
    )
]

accounts = {}
for page in pages:
    for a in page["accounts"]:
        accounts[a["account_id"]] = a

active = [a for a in accounts.values() if a["status"] == "active"]
balances = [a["balance"] for a in active]
positive = [b for b in balances if b > 0]

geo = math.exp(sum(math.log(b) for b in positive) / len(positive))
harm = len(positive) / sum(1 / b for b in positive)

answer = {
    "count": len(active),
    "sum": round(sum(balances), 2),
    "geometric_mean": round(geo, 2),
    "harmonic_mean": round(harm, 2),
}
print(len(accounts), "accounts of", total, answer)
print(await submit_answer(answer))
