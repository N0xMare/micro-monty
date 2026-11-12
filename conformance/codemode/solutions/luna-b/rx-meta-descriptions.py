import asyncio
import json
from html.parser import HTMLParser

class Meta(HTMLParser):
    def __init__(self):
        super().__init__(); self.description = None
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "meta":
            a = {k.lower(): v for k, v in attrs}
            if (a.get("name") or "").lower() == "description": self.description = a.get("content")

async def extract(i):
    data = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))
    parser = Meta(); parser.feed(data["body"])
    return str(i), parser.description

descriptions = dict(await asyncio.gather(*(extract(i) for i in range(1, 13))))
await submit_answer({"descriptions": descriptions})
