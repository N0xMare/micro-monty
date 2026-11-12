import json
from html.parser import HTMLParser

class Meta(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.value = None
    def handle_starttag(self, tag, attrs):
        a = {k.lower(): v for k, v in attrs}
        if tag.lower() == "meta" and (a.get("name") or "").lower() == "description":
            self.value = a.get("content", "")

descriptions = {}
for i in range(1, 13):
    result = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))
    parser = Meta()
    parser.feed(result["body"])
    descriptions[str(i)] = parser.value or ""
await submit_answer({"descriptions": descriptions})
