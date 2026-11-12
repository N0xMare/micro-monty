import asyncio
import json
from html.parser import HTMLParser

class Ratings(HTMLParser):
    def __init__(self):
        super().__init__(); self.values = []
    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag.lower() == "li" and "data-rating" in a:
            try: self.values.append(float(a["data-rating"]))
            except (TypeError, ValueError): pass

async def average(i):
    page = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))
    parser = Ratings(); parser.feed(page["body"])
    return str(i), round(sum(parser.values) / len(parser.values), 2) if parser.values else None

ratings = dict(await asyncio.gather(*(average(i) for i in range(1, 13))))
await submit_answer({"ratings": ratings})
