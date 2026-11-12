import asyncio
import json
from decimal import Decimal

accounts = [f"A{number:03d}" for number in range(1, 13)]
raw = await asyncio.gather(*(get_transactions({"account_id": account}) for account in accounts))
negative = {}
for account, result in zip(accounts, raw):
    net = sum((Decimal(str(transaction["amount"])) for transaction in json.loads(result)), Decimal(0))
    if net < 0:
        negative[account] = float(round(net, 2))
await submit_answer({"negative": negative})
