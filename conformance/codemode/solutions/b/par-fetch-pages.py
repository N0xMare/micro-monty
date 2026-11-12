import asyncio, json, re

ids = list(range(1, 13))
results = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids))
stock = {}
for i, r in zip(ids, results):
    resp = json.loads(r)
    m = re.search(r'<div class="stock">\s*(\d+)\s*in stock\s*</div>', resp["body"])
    if m is None:
        print("no stock found", i, resp["status"], resp["body"][:200])
        continue
    stock[i] = int(m.group(1))

out_of_stock = sorted(i for i, n in stock.items() if n == 0)
total_stock = sum(stock.values())
print(stock)
print(await submit_answer({"out_of_stock": out_of_stock, "total_stock": total_stock}))
