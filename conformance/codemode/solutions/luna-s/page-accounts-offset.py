accounts = []
offset = 0
while True:
    page = await list_accounts({"offset": offset, "limit": 10})
    accounts.extend(page["accounts"])
    offset += len(page["accounts"])
    if offset >= page["total"] or not page["accounts"]:
        break
frozen = [a["account_id"] for a in accounts if a["status"] == "frozen"]
active_usd = round(sum(a["balance"] for a in accounts if a["status"] == "active" and a["currency"] == "USD"), 2)
await submit_answer({"frozen": frozen, "active_usd_total": active_usd})
