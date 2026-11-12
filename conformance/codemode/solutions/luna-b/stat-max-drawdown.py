import asyncio
import json

accounts = [f"A{i:03d}" for i in range(1, 13)]
raw = await asyncio.gather(*(get_transactions({"account_id": account}) for account in accounts))
lowest = []
for account, result in zip(accounts, raw):
    balance = low = 0.0
    for transaction in json.loads(result):
        balance += transaction["amount"]
        low = min(low, balance)
    lowest.append((account, low))
account, low = min(lowest, key=lambda pair: pair[1])
await submit_answer({"account_id": account, "low": round(low, 2)})
