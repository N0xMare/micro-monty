import json

first = json.loads(await list_accounts({"offset": 0, "limit": 10}))
accounts = first["accounts"][:]
total = first["total"]
offsets = range(10, total, 10)
pages = await __import__("asyncio").gather(*(list_accounts({"offset": offset, "limit": 10}) for offset in offsets))
for page in pages:
    accounts.extend(json.loads(page)["accounts"])
frozen = [account["account_id"] for account in accounts if account["status"] == "frozen"]
active_usd_total = round(sum(account["balance"] for account in accounts if account["status"] == "active" and account["currency"] == "USD"), 2)
await submit_answer({"frozen": frozen, "active_usd_total": active_usd_total})
