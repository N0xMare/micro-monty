import asyncio
import re
from html.parser import HTMLParser

class Ratings(HTMLParser):
    def __init__(self):
        super().__init__()
        self.values = []
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "li":
            rating = dict(attrs).get("data-rating")
            if rating is not None:
                try: self.values.append(float(rating))
                except ValueError: pass

async def one(page_id):
    response = await http_get({"url": f"https://shop.example.com/products?id={page_id}"})
    parser = Ratings()
    parser.feed(response["body"])
    return str(page_id), round(sum(parser.values) / len(parser.values), 2) if parser.values else 0.0

ratings = dict(await asyncio.gather(*(one(i) for i in range(1, 13))))
await submit_answer({"ratings": ratings})
