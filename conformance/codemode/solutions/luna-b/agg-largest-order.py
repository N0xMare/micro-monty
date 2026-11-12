import asyncio, json

sql = "SELECT id FROM orders WHERE status='completed' ORDER BY total DESC, id ASC LIMIT 1"
order_id = json.loads(await query_database({"sql": sql}))[0]["id"]
order = json.loads(await get_order({"order_id": order_id}))
customer, products = await asyncio.gather(get_customer({"customer_id": order["customer_id"]}), *[get_product({"product_id": item["product_id"]}) for item in order["items"]])
names = sorted({json.loads(p)["name"] for p in products})
await submit_answer({"order_id": order_id, "customer": json.loads(customer)["name"], "products": names})
