import csv
import io
import json
from collections import defaultdict

products = json.loads(await list_products({}))
name_to_id = {product["name"]: product["id"] for product in products}
sales = csv.DictReader(io.StringIO(await read_file({"path": "/data/sales_2026q1.csv"})))
units = defaultdict(int)
for row in sales:
    product_name = next(row[key] for key in ("product", "product_name", "name") if key in row)
    units[name_to_id[product_name]] += int(row["units"])
await submit_answer({"units": dict(units)})
