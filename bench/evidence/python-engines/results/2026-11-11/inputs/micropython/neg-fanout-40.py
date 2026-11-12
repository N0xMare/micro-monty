async def __codemode_main():
    global asyncio, currencies, ids, orders, refunded, total
    import asyncio

    ids = range(1001, 1041)
    orders = await asyncio.gather(*(get_order({"order_id": i}) for i in ids))
    total = round(sum(o["total"] for o in orders), 2)
    refunded = sum(1 for o in orders if str(o.get("status", "")).lower() == "refunded")
    currencies = {o.get("currency") for o in orders}
    print(f"orders={len(orders)} total={total} refunded={refunded} currencies={currencies}")
    print(await submit_answer({"total": total, "refunded": refunded}))
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
