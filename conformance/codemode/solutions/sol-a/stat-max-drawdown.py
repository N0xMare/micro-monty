import asyncio
import json
from decimal import Decimal
async def low(account):
    transactions = json.loads(await get_transactions({"account_id": account}))
    balance = Decimal(0)
    lowest = balance
    for transaction in transactions:
        balance += Decimal(str(transaction["amount"]))
        lowest = min(lowest, balance)
    return account, lowest
results = await asyncio.gather(*(low(f"A{i:03d}") for i in range(1, 13)))
account, value = min(results, key=lambda result: result[1])
await submit_answer({"account_id": account, "low": float(round(value, 2))})
