import asyncio
import json
import re
import html

def text_of(fragment):
    return re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", "", fragment))).strip()

ids = range(1, 6)
results = await asyncio.gather(
    *(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids),
    return_exceptions=True,
)

products = []
for i, r in zip(ids, results):
    if isinstance(r, Exception):
        print(i, "error", r)
        continue
    resp = json.loads(r)
    body = resp["body"]
    if resp["status"] != 200:
        print(i, "status", resp["status"])
        continue
    m = re.search(r"<h1\b[^>]*>(.*?)</h1>", body, re.I | re.S)
    title = text_of(m.group(1)) if m else None

    price = None
    m = re.search(r"itemprop\s*=\s*[\"']price[\"'][^>]*\bcontent\s*=\s*[\"']([\d.,]+)", body, re.I) \
        or re.search(r"\bcontent\s*=\s*[\"']([\d.,]+)[\"'][^>]*itemprop\s*=\s*[\"']price[\"']", body, re.I)
    if m:
        price = float(m.group(1).replace(",", ""))
    else:
        m = re.search(r"class\s*=\s*[\"'][^\"']*\bprice\b[^\"']*[\"'][^>]*>(.*?)</", body, re.I | re.S)
        src = text_of(m.group(1)) if m else text_of(body)
        n = re.search(r"(\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d+(?:\.\d+)?)", src if m else
                      (re.search(r"[$€£]\s*[\d.,]+", src) or re.search(r"[\d.,]+", src)).group(0))
        if n:
            price = float(n.group(1).replace(",", ""))
    products.append({"id": i, "title": title, "price": price})

products.sort(key=lambda p: p["id"])
print(products)
print(await submit_answer({"products": products}))
