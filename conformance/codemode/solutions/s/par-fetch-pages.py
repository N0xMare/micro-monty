import re

ids = list(range(1, 13))
pages = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids))
out_of_stock = []
total_stock = 0
for i, page in zip(ids, pages):
    m = re.search(r'<div class="stock">\s*(\d+)\s*in stock\s*</div>', page["body"])
    if page["status"] != 200 or not m:
        print("unparsed", i, page["status"], page["body"][:200])
        continue
    n = int(m.group(1))
    total_stock += n
    if n == 0:
        out_of_stock.append(i)
print(out_of_stock, total_stock)
await submit_answer({"out_of_stock": sorted(out_of_stock), "total_stock": total_stock})
