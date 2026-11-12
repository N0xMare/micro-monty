import asyncio
import json
import re
from html.parser import HTMLParser
class Product(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.in_h1 = False
        self.title = []
        self.text = []
    def handle_starttag(self, tag, attrs):
        if tag == "h1":
            self.in_h1 = True
    def handle_endtag(self, tag):
        if tag == "h1":
            self.in_h1 = False
    def handle_data(self, data):
        self.text.append(data)
        if self.in_h1:
            self.title.append(data)
async def fetch(i):
    page = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))["body"]
    parser = Product()
    parser.feed(page)
    price = re.search(r"(?:\$|USD\s*)\s*([\d,]+(?:\.\d{1,2})?)", " ".join(parser.text))
    return {"id": i, "title": "".join(parser.title).strip(), "price": float(price.group(1).replace(",", ""))}
await submit_answer({"products": await asyncio.gather(*(fetch(i) for i in range(1, 6)))})
