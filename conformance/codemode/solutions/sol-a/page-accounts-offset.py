import json
from decimal import Decimal

offset = 0
frozen = []
total = Decimal(0)
while True:
    page = json.loads(await list_accounts({"offset": offset, "limit": 10}), parse_float=Decimal)
    for account in page["accounts"]:
        if account["status"] == "frozen":
            frozen.append(account["account_id"])
        if account["status"] == "active" and account["currency"] == "USD":
            total += Decimal(str(account["balance"]))
    offset += len(page["accounts"])
    if offset >= page["total"]:
        break
    if not page["accounts"]:
        raise RuntimeError("Account pagination stopped before total")
await submit_answer({"frozen": frozen, "active_usd_total": float(round(total, 2))})
