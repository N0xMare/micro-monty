import asyncio
import json
import re
from html.parser import HTMLParser

class StockParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.depth = 0
        self.text = []

    def handle_starttag(self, tag, attrs):
        if self.depth:
            self.depth += 1
        elif tag == "div" and "stock" in dict(attrs).get("class", "").split():
            self.depth = 1

    def handle_endtag(self, tag):
        if self.depth:
            self.depth -= 1

    def handle_data(self, data):
        if self.depth:
            self.text.append(data)

ids = list(range(1, 13))
results = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids))
stock = {}
for i, result in zip(ids, results):
    response = json.loads(result)
    if not 200 <= response["status"] < 300:
        raise RuntimeError(f"Product {i}: HTTP {response['status']}")
    parser = StockParser()
    parser.feed(response["body"])
    match = re.search(r'(\d+)\s+in stock', ''.join(parser.text))
    if match is None:
        raise ValueError(f"Missing stock for product {i}")
    stock[i] = int(match[1])
await submit_answer({"out_of_stock": [i for i in ids if stock[i] == 0], "total_stock": sum(stock.values())})
