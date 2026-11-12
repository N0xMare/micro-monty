import asyncio
from html.parser import HTMLParser
from statistics import mean

class Reviews(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ratings = []
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "li" and "data-rating" in attrs:
            self.ratings.append(float(attrs["data-rating"]))
responses = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in range(1, 13)))
ratings = {}
for i, response in enumerate(responses, 1):
    parser = Reviews()
    parser.feed(response["body"])
    ratings[str(i)] = round(mean(parser.ratings), 2)
await submit_answer({"ratings": ratings})
