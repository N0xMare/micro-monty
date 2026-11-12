import asyncio
import json
import re
from html.parser import HTMLParser
class Product(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.in_title = False
        self.title = []
        self.text = []
    def handle_starttag(self, tag, attrs):
        if tag == "h1":
            self.in_title = True
    def handle_endtag(self, tag):
        if tag == "h1":
            self.in_title = False
    def handle_data(self, data):
        self.text.append(data)
        if self.in_title:
            self.title.append(data)
pages = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in range(1, 6)))
products = []
for i, raw in enumerate(pages, 1):
    parser = Product()
    parser.feed(json.loads(raw)["body"])
    price = re.search(r"(?:\$|USD\s*)\s*([\d,]+(?:\.\d+)?)", " ".join(parser.text))
    products.append({"id": i, "title": "".join(parser.title).strip(), "price": float(price.group(1).replace(",", ""))})
await submit_answer({"products": products})
