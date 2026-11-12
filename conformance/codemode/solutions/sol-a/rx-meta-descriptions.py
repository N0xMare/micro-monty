import asyncio
import json
from html.parser import HTMLParser
class Description(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.value = None
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "meta" and (attrs.get("name") or "").lower() == "description":
            self.value = attrs.get("content", "")
async def fetch(i):
    response = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))
    parser = Description()
    parser.feed(response["body"])
    return str(i), parser.value
await submit_answer({"descriptions": dict(await asyncio.gather(*(fetch(i) for i in range(1, 13))))})
