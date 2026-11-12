import asyncio
from decimal import Decimal

accounts = [f"A{i:03d}" for i in range(1, 13)]
histories = await asyncio.gather(*(get_transactions({"account_id": account}) for account in accounts))
lows = {}
for account, transactions in zip(accounts, histories):
    running = low = Decimal(0)
    for transaction in transactions:
        running += Decimal(str(transaction["amount"]))
        low = min(low, running)
    lows[account] = low
account = min(lows, key=lows.get)
await submit_answer({"account_id": account, "low": float(round(lows[account], 2))})
