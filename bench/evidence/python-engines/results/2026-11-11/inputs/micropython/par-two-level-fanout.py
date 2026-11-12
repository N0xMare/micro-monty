async def __codemode_main():
    global asyncio, category_of, defaultdict, item, o, orders, product_ids, products, units
    import asyncio
    from collections import defaultdict

    orders = await asyncio.gather(*(get_order({"order_id": oid}) for oid in range(1100, 1110)))

    product_ids = sorted({item["product_id"] for o in orders for item in o["items"]})
    products = await asyncio.gather(*(get_product({"product_id": pid}) for pid in product_ids))
    category_of = {p["id"]: p["category"] for p in products}

    units = defaultdict(int)
    for o in orders:
        for item in o["items"]:
            units[category_of[item["product_id"]]] += item["qty"]

    units = dict(sorted(units.items()))
    print(units)
    await submit_answer({"units": units})
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
