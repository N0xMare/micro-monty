import asyncio
import json
from decimal import Decimal

accounts = [f"A{i:03d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_transactions({"account_id": account}) for account in accounts))
negative = {}
for account, result in zip(accounts, results):
    transactions = json.loads(result, parse_float=Decimal)
    net = sum((Decimal(str(transaction["amount"])) for transaction in transactions), Decimal(0))
    if net < 0:
        negative[account] = float(round(net, 2))
await submit_answer({"negative": negative})
