import asyncio
import json
from decimal import Decimal
ids = [f"A{i:03d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_transactions({"account_id": account_id}) for account_id in ids))
lows = {}
for account_id, raw in zip(ids, results):
    running = Decimal(0)
    low = Decimal(0)
    for transaction in json.loads(raw):
        running += Decimal(str(transaction["amount"]))
        low = min(low, running)
    lows[account_id] = low
account_id = min(lows, key=lows.get)
await submit_answer({"account_id": account_id, "low": float(round(lows[account_id], 2))})
