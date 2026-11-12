import asyncio
import json
from statistics import median

categories = ("hardware", "software", "accessories", "services")
raw = await asyncio.gather(*(list_products({"category": category}) for category in categories))
medians = {category: round(median(product["price"] for product in json.loads(result)), 2) for category, result in zip(categories, raw)}
await submit_answer({"medians": medians})
