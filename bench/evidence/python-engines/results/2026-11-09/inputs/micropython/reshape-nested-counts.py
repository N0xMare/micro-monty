async def __codemode_main():
    global asyncio, c, counts, customers, defaultdict, first, page, rest, seen, total
    import asyncio
    from collections import defaultdict

    first = await list_customers({"page": 1})
    total = first["total_pages"]
    rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, total + 1)))
    customers = []
    seen = set()
    for page in [first, *rest]:
        for c in page["customers"]:
            if c["id"] in seen:
                continue
            seen.add(c["id"])
            customers.append(c)

    counts = defaultdict(lambda: defaultdict(int))
    for c in customers:
        counts[c["region"]][c["tier"]] += 1

    counts = {r: dict(sorted(t.items())) for r, t in sorted(counts.items())}
    print(len(customers), counts)
    print(await submit_answer({"counts": counts}))
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
