import json
counts, page = {r: 0 for r in ("North", "South", "East", "West")}, 1
while True:
    result = json.loads(await list_customers({"tier": "gold", "page": page}))
    for c in result["customers"]: counts[c["region"]] += 1
    if page >= result["total_pages"]: break
    page += 1
await submit_answer({"counts": counts})
