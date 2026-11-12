from html.parser import HTMLParser
from urllib.parse import urljoin

url = "https://shop.example.com/"
response = await http_get({"url": url})
class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = set()
    def handle_starttag(self, tag, attrs):
        for key, value in attrs:
            if key == "href" and value is not None and not value.strip().lower().startswith("mailto:"):
                self.links.add(urljoin(url, value))
parser = Links()
parser.feed(response["body"])
await submit_answer({"links": sorted(parser.links)})
