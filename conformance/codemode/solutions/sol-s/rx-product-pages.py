import asyncio
import re
from html import unescape

responses = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in range(1, 6)))
products = []
for i, response in enumerate(responses, 1):
    body = response["body"]
    heading = re.search(r"<h1\b[^>]*>(.*?)</h1\s*>", body, re.I | re.S)[1]
    title = unescape(re.sub(r"<[^>]+>", "", heading)).strip()
    visible = unescape(re.sub(r"<[^>]+>", " ", body))
    price = float(re.search(r"\$\s*([\d,]+(?:\.\d+)?)", visible)[1].replace(",", ""))
    products.append({"id": i, "title": title, "price": price})
await submit_answer({"products": products})
