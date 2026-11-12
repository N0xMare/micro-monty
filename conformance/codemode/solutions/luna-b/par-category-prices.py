import asyncio
import json
from statistics import median

categories = ["hardware", "software", "accessories", "services"]
responses = await asyncio.gather(*(list_products({"category": category}) for category in categories))
medians = {category: round(median([product["price"] for product in json.loads(response)]), 2) for category, response in zip(categories, responses)}
await submit_answer({"medians": medians})
