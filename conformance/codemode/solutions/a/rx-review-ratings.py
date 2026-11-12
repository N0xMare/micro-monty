import asyncio
import json
import re

rating_re = re.compile(r"<li\b[^>]*\bdata-rating\s*=\s*[\"']?\s*([0-9]+(?:\.[0-9]+)?)", re.IGNORECASE)


async def fetch(i):
    return i, json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))


results = await asyncio.gather(*(fetch(i) for i in range(1, 13)))

ratings = {}
for i, resp in sorted(results):
    if resp.get("status") != 200:
        print("page", i, "status", resp.get("status"))
        continue
    values = [float(v) for v in rating_re.findall(resp["body"])]
    if not values:
        print("page", i, "has no reviews")
        continue
    ratings[str(i)] = round(sum(values) / len(values), 2)

print(ratings)
print(await submit_answer({"ratings": ratings}))
