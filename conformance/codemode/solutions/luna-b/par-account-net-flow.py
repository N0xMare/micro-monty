import asyncio
import json

accounts = [f"A{i:03d}" for i in range(1, 13)]
responses = await asyncio.gather(*(get_transactions({"account_id": account}) for account in accounts))
negative = {}
for account, response in zip(accounts, responses):
    net = round(sum(item["amount"] for item in json.loads(response)), 2)
    if net < 0:
        negative[account] = net
await submit_answer({"negative": negative})
