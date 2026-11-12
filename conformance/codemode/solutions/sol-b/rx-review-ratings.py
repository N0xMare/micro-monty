import asyncio
import json
import statistics
from html.parser import HTMLParser
class Reviews(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ratings = []
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "li" and "data-rating" in attrs:
            self.ratings.append(float(attrs["data-rating"]))
pages = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in range(1, 13)))
ratings = {}
for i, raw in enumerate(pages, 1):
    parser = Reviews()
    parser.feed(json.loads(raw)["body"])
    ratings[str(i)] = round(statistics.mean(parser.ratings), 2)
await submit_answer({"ratings": ratings})
