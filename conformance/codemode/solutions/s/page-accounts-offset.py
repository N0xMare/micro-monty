import asyncio

first = await list_accounts({"offset": 0, "limit": 10})
total = first["total"]
rest = await asyncio.gather(*(list_accounts({"offset": off, "limit": 10})
                              for off in range(10, total, 10)))
accounts = list(first["accounts"])
for page in rest:
    accounts.extend(page["accounts"])
print(f"fetched {len(accounts)} of {total}")

seen = set()
unique = []
for a in accounts:
    if a["account_id"] not in seen:
        seen.add(a["account_id"])
        unique.append(a)

frozen = [a["account_id"] for a in unique if a["status"] == "frozen"]
active_usd_total = round(sum(a["balance"] for a in unique
                             if a["status"] == "active" and a["currency"] == "USD"), 2)
print("frozen:", frozen)
print("active USD total:", active_usd_total)
print(await submit_answer({"frozen": frozen, "active_usd_total": active_usd_total}))
