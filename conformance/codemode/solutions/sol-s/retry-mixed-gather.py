import asyncio
from decimal import Decimal

async def fetch(account):
    for attempt in range(4):
        try:
            return await get_transactions({"account_id": account})
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise
            if attempt == 3:
                return None

accounts = ["A001", "A002", "A003", "A004"]
results = await asyncio.gather(*(fetch(a) for a in accounts))
nets = {a: float(round(sum((Decimal(str(t["amount"])) for t in rows), Decimal(0)), 2)) for a, rows in zip(accounts, results) if rows is not None}
await submit_answer({"nets": nets, "failed": [a for a, r in zip(accounts, results) if r is None]})
