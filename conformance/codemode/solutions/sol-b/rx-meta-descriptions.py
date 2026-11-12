import asyncio
import json
from html.parser import HTMLParser
class Description(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.description = ""
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "meta" and (attrs.get("name") or "").lower() == "description":
            self.description = attrs.get("content", "")
pages = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in range(1, 13)))
descriptions = {}
for i, raw in enumerate(pages, 1):
    parser = Description()
    parser.feed(json.loads(raw)["body"])
    descriptions[str(i)] = parser.description
await submit_answer({"descriptions": descriptions})
