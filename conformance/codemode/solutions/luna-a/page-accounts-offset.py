import json

accounts = []
offset = 0
while True:
    page = json.loads(await list_accounts({"offset": offset, "limit": 10}))
    batch = page["accounts"]
    accounts.extend(batch)
    offset += len(batch)
    if offset >= page["total"] or not batch:
        break
frozen = [account["account_id"] for account in accounts if account["status"] == "frozen"]
active_usd_total = round(sum(account["balance"] for account in accounts if account["status"] == "active" and account["currency"] == "USD"), 2)
await submit_answer({"frozen": frozen, "active_usd_total": active_usd_total})
