import json
from html import unescape
from html.parser import HTMLParser
from urllib.parse import parse_qsl, urljoin, urlsplit

response = json.loads(await http_get({"url": "https://shop.example.com/"}))
class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.hrefs = []
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "a":
            href = dict(attrs).get("href")
            if href is not None:
                self.hrefs.append(unescape(href))
p = Links()
p.feed(response["body"])
links = []
for href in p.hrefs:
    parts = urlsplit(urljoin("https://shop.example.com/", href))
    if parts.scheme == "https":
        links.append({"host": parts.hostname, "path": parts.path or "/", "query": dict(parse_qsl(parts.query, keep_blank_values=True))})
await submit_answer({"links": links})
