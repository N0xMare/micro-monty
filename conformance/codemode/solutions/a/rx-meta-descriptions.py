import asyncio
import json
from html.parser import HTMLParser


class MetaParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.description = None

    def handle_starttag(self, tag, attrs):
        if tag.lower() != "meta" or self.description is not None:
            return
        a = {k.lower(): (v or "") for k, v in attrs}
        if a.get("name", "").strip().lower() == "description" and "content" in a:
            self.description = a["content"]

    handle_startendtag = handle_starttag


async def fetch(i):
    resp = json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))
    return i, resp


results = await asyncio.gather(*(fetch(i) for i in range(1, 13)))

descriptions = {}
for i, resp in results:
    if resp.get("status") != 200:
        print("page", i, "status", resp.get("status"))
        continue
    p = MetaParser()
    p.feed(resp["body"])
    p.close()
    if p.description is None:
        print("page", i, "has no meta description")
        continue
    descriptions[str(i)] = p.description.strip()

print(descriptions)
print(await submit_answer({"descriptions": descriptions}))
