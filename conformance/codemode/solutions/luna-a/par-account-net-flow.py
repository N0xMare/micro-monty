import asyncio
import json

accounts = [f"A{i:03d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_transactions({"account_id": account}) for account in accounts))
negative = {}
for account, result in zip(accounts, results):
    net = sum(item["amount"] for item in json.loads(result))
    if net < 0:
        negative[account] = round(net, 2)
await submit_answer({"negative": negative})
