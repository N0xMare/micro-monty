import asyncio

account_ids = [f"A{i:03d}" for i in range(1, 13)]
transactions = await asyncio.gather(*(get_transactions({"account_id": account_id}) for account_id in account_ids))
negative = {}
for account_id, rows in zip(account_ids, transactions):
    net = round(sum(row["amount"] for row in rows), 2)
    if net < 0:
        negative[account_id] = net
await submit_answer({"negative": negative})
