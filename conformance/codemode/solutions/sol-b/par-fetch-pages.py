import asyncio
import json
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
raw = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={page_id}"}) for page_id in ids))
stocks = {}
for page_id, result in zip(ids, raw):
    parser = StockParser()
    parser.feed(json.loads(result)["body"])
    stocks[page_id] = int("".join(parser.text).split()[0])
await submit_answer({"out_of_stock": [page_id for page_id in ids if stocks[page_id] == 0], "total_stock": sum(stocks.values())})
