import json
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit, parse_qsl
class Links(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hrefs = []
    def handle_starttag(self, tag, attrs):
        if tag == "a":
            for key, value in attrs:
                if key == "href" and value is not None:
                    self.hrefs.append(value)
                    break
response = json.loads(await http_get({"url": "https://shop.example.com/"}))
parser = Links()
parser.feed(response["body"])
links = []
for href in parser.hrefs:
    url = urlsplit(urljoin("https://shop.example.com/", href))
    if url.scheme == "https":
        links.append({"host": url.hostname, "path": url.path, "query": dict(parse_qsl(url.query, keep_blank_values=True))})
await submit_answer({"links": links})
