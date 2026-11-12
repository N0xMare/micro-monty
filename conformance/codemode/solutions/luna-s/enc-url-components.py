import html
from html.parser import HTMLParser
from urllib.parse import parse_qsl, urljoin, urlsplit

class LinkParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hrefs = []

    def handle_starttag(self, tag, attrs):
        if tag.lower() == "a":
            for key, value in attrs:
                if key.lower() == "href" and value is not None:
                    self.hrefs.append(html.unescape(value))
                    break

response = await http_get({"url": "https://shop.example.com/"})
parser = LinkParser()
parser.feed(response["body"])
links = []
for href in parser.hrefs:
    parts = urlsplit(urljoin("https://shop.example.com/", href))
    if parts.scheme == "https":
        links.append({"host": parts.hostname or "", "path": parts.path or "/", "query": dict(parse_qsl(parts.query, keep_blank_values=True))})
await submit_answer({"links": links})
