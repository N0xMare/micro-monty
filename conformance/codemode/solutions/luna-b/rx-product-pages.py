import asyncio
import json
import re
from html.parser import HTMLParser

class Product(HTMLParser):
    def __init__(self):
        super().__init__(); self.in_h1 = False; self.title = ""; self.price_text = ""
    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag.lower() == "h1": self.in_h1 = True
        if "price" in (a.get("class") or "").lower(): self.price_text += " "
    def handle_endtag(self, tag):
        if tag.lower() == "h1": self.in_h1 = False
    def handle_data(self, data):
        if self.in_h1: self.title += data
        self.price_text += data

async def extract(i):
    page = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))
    parser = Product(); parser.feed(page["body"])
    match = re.search(r"(?:\$|USD\s*)([\d,]+(?:\.\d{2})?)", parser.price_text, re.I)
    return {"id": i, "title": parser.title.strip(), "price": float(match.group(1).replace(",", "")) if match else None}

products = await asyncio.gather(*(extract(i) for i in range(1, 6)))
await submit_answer({"products": products})
