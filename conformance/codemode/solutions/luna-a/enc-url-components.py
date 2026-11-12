import json
from html import unescape
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit, parse_qsl
r = json.loads(await http_get({"url": "https://shop.example.com/"}))
class Parser(HTMLParser):
    def __init__(self): super().__init__(); self.hrefs = []
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "a":
            href = dict(attrs).get("href")
            if href is not None: self.hrefs.append(href)
parser = Parser(); parser.feed(r["body"]); links = []
for href in parser.hrefs:
    p = urlsplit(urljoin("https://shop.example.com/", unescape(href)))
    if p.scheme != "https": continue
    query = {}
    for k, v in parse_qsl(p.query, keep_blank_values=True):
        if k in query:
            if not isinstance(query[k], list): query[k] = [query[k]]
            query[k].append(v)
        else: query[k] = v
    links.append({"host": p.netloc, "path": p.path or "/", "query": query})
await submit_answer({"links": links})
