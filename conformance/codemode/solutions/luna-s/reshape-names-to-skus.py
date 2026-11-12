import csv
import io
sales = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/sales_2026q1.csv"}))))
products = await list_products({})
ids = {product["name"]: product["id"] for product in products}
units = {}
for sale in sales:
    product_id = ids[sale["product"]]
    units[product_id] = units.get(product_id, 0) + int(sale["units"])
await submit_answer({"units": units})
