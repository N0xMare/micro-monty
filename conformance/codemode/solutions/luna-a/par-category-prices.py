import asyncio
import json
from statistics import median

categories = ["hardware", "software", "accessories", "services"]
results = await asyncio.gather(*(list_products({"category": category}) for category in categories))
medians = {category: round(median(product["price"] for product in json.loads(result)), 2) for category, result in zip(categories, results)}
await submit_answer({"medians": medians})
