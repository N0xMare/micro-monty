import json
import re
from html.parser import HTMLParser

class Product(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.capture = None
        self.parts = {"h1": [], "price": []}
    def handle_starttag(self, tag, attrs):
        a = {k.lower(): (v or "") for k, v in attrs}
        if tag.lower() == "h1":
            self.capture = "h1"
        elif tag.lower() in ("span", "div", "p") and ("price" in a.get("class", "").lower() or a.get("itemprop", "").lower() == "price"):
            self.capture = "price"
        if tag.lower() == "meta" and a.get("itemprop", "").lower() == "price":
            self.parts["price"].append(a.get("content", ""))
    def handle_data(self, data):
        if self.capture:
            self.parts[self.capture].append(data)
    def handle_endtag(self, tag):
        if tag.lower() == "h1" and self.capture == "h1":
            self.capture = None
        elif tag.lower() in ("span", "div", "p") and self.capture == "price":
            self.capture = None

products = []
for i in range(1, 6):
    result = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))
    parser = Product()
    parser.feed(result["body"])
    title = " ".join(" ".join(parser.parts["h1"]).split())
    m = re.search(r"\d+(?:,\d{3})*(?:\.\d+)?", " ".join(parser.parts["price"]))
    products.append({"id": i, "title": title, "price": float(m.group().replace(",", "")) if m else 0.0})
await submit_answer({"products": products})
