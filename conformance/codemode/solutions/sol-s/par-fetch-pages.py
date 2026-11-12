import asyncio
import re

ids = list(range(1, 13))
responses = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={page_id}"}) for page_id in ids))
stocks = {}
for page_id, response in zip(ids, responses):
    if not 200 <= response["status"] < 300:
        raise RuntimeError(f"Page {page_id}: HTTP {response['status']}")
    match = re.search(r'<div\s+class=[\"\']stock[\"\']\s*>\s*(\d+)\s+in stock\s*</div>', response["body"])
    if match is None:
        raise ValueError(f"Missing stock on page {page_id}")
    stocks[page_id] = int(match[1])
await submit_answer({"out_of_stock": sorted(page_id for page_id, stock in stocks.items() if stock == 0), "total_stock": sum(stocks.values())})
