import asyncio
import statistics

first = await list_accounts({"offset": 0, "limit": 10})
total = first["total"]
pages = await asyncio.gather(
    *[list_accounts({"offset": off, "limit": 10}) for off in range(10, total, 10)]
)

accounts = {}
for page in [first, *pages]:
    for a in page["accounts"]:
        accounts[a["account_id"]] = a
print("total:", total, "fetched:", len(accounts))

active = [a for a in accounts.values() if a["status"] == "active"]
balances = [a["balance"] for a in active]
positive = [b for b in balances if b > 0]

answer = {
    "count": len(active),
    "sum": round(sum(balances), 2),
    "geometric_mean": round(statistics.geometric_mean(positive), 2),
    "harmonic_mean": round(statistics.harmonic_mean(positive), 2),
}
print(answer, "positive:", len(positive))
await submit_answer(answer)
