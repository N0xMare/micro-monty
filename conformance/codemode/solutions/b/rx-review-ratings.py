import asyncio
import json
import re

ids = range(1, 13)
results = await asyncio.gather(
    *(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids),
    return_exceptions=True,
)

ratings = {}
for i, r in zip(ids, results):
    if isinstance(r, Exception):
        print(i, "error", r)
        continue
    resp = json.loads(r)
    if resp["status"] != 200:
        print(i, "status", resp["status"])
        continue
    values = [float(v) for v in re.findall(
        r"<li\b[^>]*\bdata-rating\s*=\s*[\"']?\s*(\d+(?:\.\d+)?)", resp["body"], re.I)]
    if not values:
        print(i, "no reviews")
        continue
    ratings[str(i)] = round(sum(values) / len(values), 2)

print(ratings)
print(await submit_answer({"ratings": ratings}))
