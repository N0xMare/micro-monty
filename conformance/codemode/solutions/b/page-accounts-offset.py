import json

accounts, offset, total = [], 0, None
while total is None or offset < total:
    page = json.loads(await list_accounts({"offset": offset, "limit": 10}))
    total = page["total"]
    batch = page["accounts"]
    if not batch:
        break
    accounts.extend(batch)
    offset += len(batch)
print(len(accounts), "of", total, "accounts")
frozen = [a["account_id"] for a in accounts if a["status"] == "frozen"]
active_usd_total = round(sum(a["balance"] for a in accounts if a["status"] == "active" and a["currency"] == "USD"), 2)
print(frozen, active_usd_total)
print(await submit_answer({"frozen": frozen, "active_usd_total": active_usd_total}))
