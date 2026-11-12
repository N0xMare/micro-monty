import asyncio
import json
from html.parser import HTMLParser
class Ratings(HTMLParser):
    def __init__(self):
        super().__init__()
        self.values = []
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "li" and "data-rating" in attrs:
            self.values.append(float(attrs["data-rating"]))
async def fetch(i):
    page = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))["body"]
    parser = Ratings()
    parser.feed(page)
    return str(i), round(sum(parser.values) / len(parser.values), 2)
await submit_answer({"ratings": dict(await asyncio.gather(*(fetch(i) for i in range(1, 13))))})
