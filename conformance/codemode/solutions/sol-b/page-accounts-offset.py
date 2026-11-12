import asyncio
import json
from decimal import Decimal

first = json.loads(await list_accounts({"offset": 0, "limit": 10}))
rest = await asyncio.gather(*(list_accounts({"offset": offset, "limit": 10}) for offset in range(10, first["total"], 10)))
accounts = first["accounts"] + [account for raw in rest for account in json.loads(raw)["accounts"]]
total = sum((Decimal(str(account["balance"])) for account in accounts if account["status"] == "active" and account["currency"] == "USD"), Decimal(0))
await submit_answer({"frozen": [account["account_id"] for account in accounts if account["status"] == "frozen"], "active_usd_total": float(round(total, 2))})
