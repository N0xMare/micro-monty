low = None
for product_id in ("P01", "P02", "P03", "P08", "P04", "P05"):
    product = await get_product({"product_id": product_id})
    if product["stock"] < 10:
        low = product_id
        break
await submit_answer({"product_id": low})
