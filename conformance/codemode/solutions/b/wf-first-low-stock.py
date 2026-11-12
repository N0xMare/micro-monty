import json

found = None
for pid in ["P01", "P02", "P03", "P08", "P04", "P05"]:
    product = json.loads(await get_product({"product_id": pid}))
    print(pid, product["stock"])
    if product["stock"] < 10:
        found = pid
        break

print(await submit_answer({"product_id": found}))
