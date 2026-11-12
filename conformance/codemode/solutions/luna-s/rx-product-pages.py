import asyncio
import re
from html.parser import HTMLParser

class Product(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.title = []
        self.price_text = []
        self.capture = None
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        classes = (attrs.get("class") or "").lower().split()
        if tag.lower() == "h1":
            self.capture = "title"
        elif "price" in classes or attrs.get("itemprop") == "price":
            self.capture = "price"
    def handle_endtag(self, tag):
        if tag.lower() in ("h1", "span", "p", "div"):
            self.capture = None
    def handle_data(self, data):
        if self.capture == "title": self.title.append(data)
        elif self.capture == "price": self.price_text.append(data)

async def one(page_id):
    result = await http_get({"url": f"https://shop.example.com/products?id={page_id}"})
    parser = Product()
    parser.feed(result["body"])
    match = re.search(r"\d+(?:,\d{3})*(?:\.\d+)?", " ".join(parser.price_text))
    return {"id": page_id, "title": " ".join("".join(parser.title).split()), "price": float(match.group().replace(",", "")) if match else 0.0}

products = await asyncio.gather(*(one(i) for i in range(1, 6)))
await submit_answer({"products": products})
