import asyncio
ids = [1001, 1500, 1002, 2000]
async def lookup(order_id):
    try:
        await get_order({"order_id": order_id})
        return order_id, None
    except Exception as error:
        return order_id, str(error)
results = await asyncio.gather(*(lookup(order_id) for order_id in ids))
ok = [order_id for order_id, error in results if error is None]
errors = {str(order_id): error for order_id, error in results if error is not None}
await submit_answer({"ok": ok, "errors": errors})
