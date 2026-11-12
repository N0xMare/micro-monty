import json
import asyncio
import re

ids = list(range(1, 13))
results = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids))
out_of_stock = []
total = 0
for i, r in zip(ids, results):
    resp = json.loads(r)
    m = re.search(r'<div class="stock">\s*([\d,]+)\s*in stock\s*</div>', resp["body"])
    if resp["status"] != 200 or not m:
        print("problem with page", i, resp["status"], resp["body"][:200])
        continue
    n = int(m.group(1).replace(",", ""))
    total += n
    if n == 0:
        out_of_stock.append(i)
print(out_of_stock, total)
await submit_answer({"out_of_stock": sorted(out_of_stock), "total_stock": total})
