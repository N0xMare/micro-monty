import json
from html.parser import HTMLParser
from urllib.parse import urljoin

class Links(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hrefs = []
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "a":
            href = dict(attrs).get("href")
            if href is not None:
                self.hrefs.append(href)

page_url = "https://shop.example.com/"
page = json.loads(await http_get({"url": page_url}))
parser = Links()
parser.feed(page["body"])
links = sorted({urljoin(page_url, href) for href in parser.hrefs if not href.strip().lower().startswith("mailto:")})
await submit_answer({"links": links})
