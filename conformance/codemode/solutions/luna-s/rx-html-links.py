from html.parser import HTMLParser
from urllib.parse import urljoin
import re

url = "https://shop.example.com/"
page = await http_get({"url": url})

class Links(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hrefs = []
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "a":
            for key, value in attrs:
                if key.lower() == "href" and value is not None:
                    self.hrefs.append(value)

parser = Links()
parser.feed(page["body"])
targets = sorted({urljoin(url, href) for href in parser.hrefs if not href.strip().lower().startswith("mailto:")})
await submit_answer({"links": targets})
