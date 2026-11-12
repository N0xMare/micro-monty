import json
from html.parser import HTMLParser
from urllib.parse import urljoin
url = "https://shop.example.com/"
class Links(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.links = set()
    def handle_starttag(self, tag, attrs):
        for key, value in attrs:
            if key == "href" and value is not None and not value.strip().lower().startswith("mailto:"):
                self.links.add(urljoin(url, value.strip()))
parser = Links()
parser.feed(json.loads(await http_get({"url": url}))["body"])
await submit_answer({"links": sorted(parser.links)})
