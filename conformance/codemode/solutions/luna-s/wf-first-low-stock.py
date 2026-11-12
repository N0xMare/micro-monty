product_id = None
for candidate in ("P01", "P02", "P03", "P08", "P04", "P05"):
    product = await get_product({"product_id": candidate})
    if product["stock"] < 10:
        product_id = candidate
        break
await submit_answer({"product_id": product_id})
