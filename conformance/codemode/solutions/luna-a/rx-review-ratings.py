import json
from html.parser import HTMLParser

class Ratings(HTMLParser):
    def __init__(self):
        super().__init__()
        self.values = []
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "li":
            rating = dict(attrs).get("data-rating")
            if rating is not None:
                self.values.append(float(rating))

ratings = {}
for i in range(1, 13):
    page = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))
    parser = Ratings()
    parser.feed(page["body"])
    ratings[str(i)] = round(sum(parser.values) / len(parser.values), 2) if parser.values else 0.0
await submit_answer({"ratings": ratings})
