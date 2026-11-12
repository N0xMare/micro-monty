import asyncio
ids = [4, 77, 12, 300, 25]
async def lookup(customer_id):
    try:
        result = await get_customer({"customer_id": customer_id})
        return customer_id, result["name"], False
    except LookupError:
        return customer_id, None, True
results = await asyncio.gather(*(lookup(customer_id) for customer_id in ids))
found = {str(customer_id): name for customer_id, name, missing in results if not missing}
not_found = [customer_id for customer_id, name, missing in results if missing]
await submit_answer({"found": found, "not_found": not_found})
