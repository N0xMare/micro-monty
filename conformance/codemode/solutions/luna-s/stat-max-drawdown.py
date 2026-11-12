import asyncio

account_ids = [f"A{i:03d}" for i in range(1, 13)]
transactions = await asyncio.gather(*(get_transactions({"account_id": account_id}) for account_id in account_ids))
lows = {}
for account_id, rows in zip(account_ids, transactions):
    balance = 0.0
    low = 0.0
    for row in rows:
        balance += row["amount"]
        low = min(low, balance)
    lows[account_id] = low
account_id = min(account_ids, key=lambda key: (lows[key], key))
await submit_answer({"account_id": account_id, "low": round(lows[account_id], 2)})
