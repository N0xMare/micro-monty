import asyncio
import json
import re
from html.parser import HTMLParser

class Mailto(HTMLParser):
    def __init__(self):
        super().__init__(); self.addresses = []
    def handle_starttag(self, tag, attrs):
        href = dict(attrs).get("href", "")
        if href.lower().startswith("mailto:"):
            self.addresses.extend(re.findall(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", href[7:]))

async def emails(url):
    page = json.loads(await http_get({"url": url}))
    body = page["body"]
    found = set(re.findall(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+", body))
    parser = Mailto(); parser.feed(body); found.update(parser.addresses)
    return found

results = await asyncio.gather(emails("https://shop.example.com/"), emails("https://shop.example.com/about"))
await submit_answer({"emails": sorted(set.union(*results))})
