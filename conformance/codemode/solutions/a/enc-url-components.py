import json
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit, parse_qsl

page = "https://shop.example.com/"
resp = json.loads(await http_get({"url": page}))
print(resp["status"], resp["content_type"])

class Links(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hrefs = []
    def handle_starttag(self, tag, attrs):
        if tag == "a":
            for k, v in attrs:
                if k == "href" and v is not None:
                    self.hrefs.append(v)
    handle_startendtag = handle_starttag

p = Links()
p.feed(resp["body"])
links = []
for href in p.hrefs:  # attribute values are already entity-decoded (&amp; -> &)
    u = urlsplit(urljoin(page, href.strip()))
    if u.scheme != "https":
        continue
    links.append({"host": u.hostname or "", "path": u.path, "query": dict(parse_qsl(u.query, keep_blank_values=True))})
print(p.hrefs)
print(links)
print(await submit_answer({"links": links}))
