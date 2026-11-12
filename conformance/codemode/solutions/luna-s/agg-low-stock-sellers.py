products, rows = await asyncio.gather(list_products({}), query_database({"sql": "SELECT oi.product_id, SUM(oi.qty) AS units FROM order_items oi JOIN orders o ON o.id = oi.order_id WHERE o.status = 'completed' GROUP BY oi.product_id"}))
sold = {row["product_id"]: int(row["units"]) for row in rows}
product_ids = sorted(product["id"] for product in products if int(product["stock"]) < 10 and sold.get(product["id"], 0) >= 5)
await submit_answer({"product_ids": product_ids})
