import asyncio, json

first = json.loads(await list_customers({"page": 1}))
pages = [first]
if first["total_pages"] > 1:
    rest = await asyncio.gather(*[list_customers({"page": p}) for p in range(2, first["total_pages"] + 1)])
    pages += [json.loads(r) for r in rest]

seen = {}
for pg in pages:
    for c in pg["customers"]:
        seen[c["id"]] = c

counts = {}
for c in seen.values():
    region, tier = c.get("region"), c.get("tier")
    counts.setdefault(region, {})
    counts[region][tier] = counts[region].get(tier, 0) + 1

print(len(seen), "customers")
print(json.dumps(counts, indent=1, sort_keys=True))
print(await submit_answer({"counts": counts}))
