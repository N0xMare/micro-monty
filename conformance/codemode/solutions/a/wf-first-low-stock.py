import json

found = None
for pid in ["P01", "P02", "P03", "P08", "P04", "P05"]:
    product = json.loads(await get_product({"product_id": pid}))
    if product["stock"] < 10:
        found = pid
        break

await submit_answer({"product_id": found})
