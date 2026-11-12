import asyncio
from statistics import median

categories = ("hardware", "software", "accessories", "services")
results = await asyncio.gather(*(list_products({"category": category}) for category in categories))
await submit_answer({"medians": {category: round(median(product["price"] for product in products), 2) for category, products in zip(categories, results)}})
