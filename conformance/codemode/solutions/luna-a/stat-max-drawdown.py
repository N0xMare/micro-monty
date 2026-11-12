import json
import asyncio

ids = [f"A{i:03d}" for i in range(1, 13)]
transactions = await asyncio.gather(*(get_transactions({"account_id": aid}) for aid in ids))
lowest = {}
for aid, raw in zip(ids, transactions):
    running = low = 0.0
    for tx in json.loads(raw):
        running += float(tx["amount"])
        low = min(low, running)
    lowest[aid] = low
account = min(ids, key=lambda aid: (lowest[aid], aid))
await submit_answer({"account_id": account, "low": round(lowest[account], 2)})
