import asyncio
import json
import statistics
first = json.loads(await list_accounts({"offset": 0, "limit": 10}))
pages = await asyncio.gather(*(list_accounts({"offset": offset, "limit": 10}) for offset in range(10, first["total"], 10)))
accounts = first["accounts"] + [account for raw in pages for account in json.loads(raw)["accounts"]]
balances = [a["balance"] for a in accounts if a["status"] == "active"]
positive = [value for value in balances if value > 0]
await submit_answer({"count": len(balances), "sum": round(sum(balances), 2), "geometric_mean": round(statistics.geometric_mean(positive), 2), "harmonic_mean": round(statistics.harmonic_mean(positive), 2)})
