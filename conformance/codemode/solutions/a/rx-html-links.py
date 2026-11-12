import json
import html
from html.parser import HTMLParser
from urllib.parse import urljoin

page_url = "https://shop.example.com/"
resp = json.loads(await http_get({"url": page_url}))
body = resp["body"]


class LinkParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hrefs = []
        self.base = None

    def handle_starttag(self, tag, attrs):
        for name, value in attrs:
            if name.lower() != "href" or value is None:
                continue
            if tag.lower() == "base" and self.base is None:
                self.base = value
            else:
                self.hrefs.append(value)

    handle_startendtag = handle_starttag


p = LinkParser()
p.feed(body)
p.close()

base = urljoin(page_url, html.unescape(p.base.strip())) if p.base else page_url

links = set()
for href in p.hrefs:
    href = html.unescape(href).strip()  # attrs are already decoded; unescape is idempotent for normal URLs
    if not href or href.lower().startswith("mailto:"):
        continue
    absolute = urljoin(base, href)
    links.add(absolute)

links = sorted(links)
print(resp["status"], links)
print(await submit_answer({"links": links}))
