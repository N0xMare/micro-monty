import asyncio
import json
import re

page_ids = list(range(1, 13))
responses = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={page_id}"}) for page_id in page_ids))
stock_by_id = {}
for page_id, result in zip(page_ids, responses):
    body = json.loads(result)["body"]
    match = re.search(r'<div\s+class="stock">\s*(\d+)\s+in stock\s*</div>', body, re.IGNORECASE)
    stock_by_id[page_id] = int(match.group(1))
await submit_answer({"out_of_stock": sorted(page_id for page_id, stock in stock_by_id.items() if stock == 0), "total_stock": sum(stock_by_id.values())})
