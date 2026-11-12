import asyncio
from html.parser import HTMLParser

ids = list(range(1, 13))
resps = await asyncio.gather(
    *[http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids],
    return_exceptions=True,
)

class MetaParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.description = None

    def handle_starttag(self, tag, attrs):
        if tag.lower() != "meta" or self.description is not None:
            return
        a = {k.lower(): v for k, v in attrs}
        if (a.get("name") or "").strip().lower() == "description" and a.get("content") is not None:
            self.description = a["content"].strip()

    handle_startendtag = handle_starttag

descriptions = {}
for i, r in zip(ids, resps):
    if isinstance(r, Exception):
        print(i, "error:", r)
        continue
    if r["status"] != 200:
        print(i, "status", r["status"])
        continue
    p = MetaParser()
    p.feed(r["body"])
    p.close()
    if p.description is None:
        print(i, "no meta description")
        continue
    descriptions[str(i)] = p.description

print(descriptions)
await submit_answer({"descriptions": descriptions})
