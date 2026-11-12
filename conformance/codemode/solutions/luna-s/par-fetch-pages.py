import asyncio
import re

ids = list(range(1, 13))
pages = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids))
stocks = []
for page in pages:
    match = re.search(r'<div\s+class="stock">\s*(\d+)\s+in stock\s*</div>', page["body"], re.I)
    stocks.append(int(match.group(1)))
await submit_answer({"out_of_stock": [i for i, stock in zip(ids, stocks) if stock == 0], "total_stock": sum(stocks)})
