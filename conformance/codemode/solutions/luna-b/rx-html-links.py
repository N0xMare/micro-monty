import json
from html.parser import HTMLParser
from urllib.parse import urljoin

class Links(HTMLParser):
    def __init__(self):
        super().__init__(); self.hrefs = []
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "a":
            href = dict(attrs).get("href")
            if href is not None: self.hrefs.append(href)

base = "https://shop.example.com/"
page = json.loads(await http_get({"url": base}))
parser = Links(); parser.feed(page["body"])
links = sorted({urljoin(base, h) for h in parser.hrefs if not h.strip().lower().startswith("mailto:")})
await submit_answer({"links": links})
