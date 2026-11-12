import asyncio, json
from decimal import Decimal

async def fetch(account):
    for attempt in range(4):
        try:
            raw = await get_transactions({"account_id": account})
        except (TimeoutError, RuntimeError) as exc:
            if not (isinstance(exc, TimeoutError) or "503" in str(exc)):
                raise
            if attempt == 3:
                return None
        else:
            return json.loads(raw, parse_float=Decimal)

accounts = ["A001", "A002", "A003", "A004"]
results = await asyncio.gather(*(fetch(a) for a in accounts))
nets = {a: float(round(sum((r["amount"] for r in rows), Decimal(0)), 2)) for a, rows in zip(accounts, results) if rows is not None}
await submit_answer({"nets": nets, "failed": [a for a, rows in zip(accounts, results) if rows is None]})
