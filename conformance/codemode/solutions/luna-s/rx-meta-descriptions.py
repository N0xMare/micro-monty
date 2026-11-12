import asyncio
from html.parser import HTMLParser

class MetaDescription(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.value = None
    def handle_starttag(self, tag, attrs):
        attrs = {k.lower(): v for k, v in attrs}
        if tag.lower() == "meta" and (attrs.get("name") or "").lower() == "description":
            self.value = attrs.get("content", "")

async def one(page_id):
    response = await http_get({"url": f"https://shop.example.com/products?id={page_id}"})
    parser = MetaDescription()
    parser.feed(response["body"])
    return str(page_id), parser.value or ""

descriptions = dict(await asyncio.gather(*(one(i) for i in range(1, 13))))
await submit_answer({"descriptions": descriptions})
