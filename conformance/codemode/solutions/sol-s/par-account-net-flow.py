import asyncio
from decimal import Decimal

accounts = [f"A{i:03d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_transactions({"account_id": account}) for account in accounts))
flows = {account: sum((Decimal(str(row["amount"])) for row in rows), Decimal(0)) for account, rows in zip(accounts, results)}
await submit_answer({"negative": {account: float(round(flow, 2)) for account, flow in flows.items() if flow < 0}})
