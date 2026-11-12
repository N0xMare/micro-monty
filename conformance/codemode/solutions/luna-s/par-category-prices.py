import asyncio
from statistics import median

categories = ["hardware", "software", "accessories", "services"]
products = await asyncio.gather(*(list_products({"category": category}) for category in categories))
medians = {category: round(median(p["price"] for p in rows), 2) for category, rows in zip(categories, products)}
await submit_answer({"medians": medians})
