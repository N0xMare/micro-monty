import asyncio
import json
from statistics import median
from decimal import Decimal

categories = ["hardware", "software", "accessories", "services"]
results = await asyncio.gather(*(list_products({"category": category}) for category in categories))
medians = {category: float(round(median(Decimal(str(product["price"])) for product in json.loads(result, parse_float=Decimal)), 2)) for category, result in zip(categories, results)}
await submit_answer({"medians": medians})
