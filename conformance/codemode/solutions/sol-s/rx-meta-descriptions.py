import asyncio
from html.parser import HTMLParser

class Description(HTMLParser):
    def __init__(self):
        super().__init__()
        self.description = None
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "meta" and (attrs.get("name") or "").lower() == "description":
            self.description = attrs.get("content", "")
responses = await asyncio.gather(*(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in range(1, 13)))
descriptions = {}
for i, response in enumerate(responses, 1):
    parser = Description()
    parser.feed(response["body"])
    descriptions[str(i)] = parser.description
await submit_answer({"descriptions": descriptions})
