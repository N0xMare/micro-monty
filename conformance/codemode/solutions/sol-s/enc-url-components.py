from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit, parse_qsl

class LinkParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hrefs = []

    def handle_starttag(self, tag, attrs):
        if tag == "a":
            for name, value in attrs:
                if name == "href" and value is not None:
                    self.hrefs.append(value)
                    break

base = "https://shop.example.com/"
response = await http_get({"url": base})
parser = LinkParser()
parser.feed(response["body"])
links = []
for href in parser.hrefs:
    url = urlsplit(urljoin(base, href))
    if url.scheme == "https":
        links.append({"host": url.netloc, "path": url.path, "query": dict(parse_qsl(url.query, keep_blank_values=True))})
await submit_answer({"links": links})
