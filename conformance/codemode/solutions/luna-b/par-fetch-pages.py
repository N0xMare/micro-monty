import asyncio
import json
import re

ids = list(range(1, 13))
responses = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={page_id}"}) for page_id in ids))
out_of_stock = []
total_stock = 0
for page_id, response in zip(ids, responses):
    body = json.loads(response)["body"]
    match = re.search(r'<div class="stock">\s*(\d+)\s+in stock\s*</div>', body)
    if match:
        stock = int(match.group(1))
        total_stock += stock
        if stock == 0:
            out_of_stock.append(page_id)
await submit_answer({"out_of_stock": out_of_stock, "total_stock": total_stock})
