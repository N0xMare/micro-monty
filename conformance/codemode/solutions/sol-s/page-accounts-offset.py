from decimal import Decimal

offset = 0
frozen = []
total = Decimal(0)
while True:
    result = await list_accounts({"offset": offset, "limit": 10})
    for account in result["accounts"]:
        if account["status"] == "frozen":
            frozen.append(account["account_id"])
        if account["status"] == "active" and account["currency"] == "USD":
            total += Decimal(str(account["balance"]))
    offset += len(result["accounts"])
    if offset >= result["total"]:
        break
    if not result["accounts"]:
        raise RuntimeError("Empty page before total accounts reached")
await submit_answer({"frozen": frozen, "active_usd_total": float(round(total, 2))})
