import asyncio
from html.parser import HTMLParser

ids = list(range(1, 13))
resps = await asyncio.gather(
    *[http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids],
    return_exceptions=True,
)

class ReviewParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ratings = []

    def handle_starttag(self, tag, attrs):
        if tag.lower() != "li":
            return
        for k, v in attrs:
            if k.lower() == "data-rating" and v is not None:
                try:
                    self.ratings.append(float(v.strip()))
                except ValueError:
                    print("bad rating:", v)

ratings = {}
for i, r in zip(ids, resps):
    if isinstance(r, Exception):
        print(i, "error:", r)
        continue
    if r["status"] != 200:
        print(i, "status", r["status"])
        continue
    p = ReviewParser()
    p.feed(r["body"])
    p.close()
    if not p.ratings:
        print(i, "no reviews")
        continue
    ratings[str(i)] = round(sum(p.ratings) / len(p.ratings), 2)
    print(i, p.ratings, ratings[str(i)])

await submit_answer({"ratings": ratings})
