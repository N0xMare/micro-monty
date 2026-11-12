from html.parser import HTMLParser
from urllib.parse import urljoin

PAGE = "https://shop.example.com/"
resp = await http_get({"url": PAGE})
print(resp["status"], resp["content_type"], len(resp["body"]))

class LinkParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.base = None
        self.hrefs = []

    def handle_starttag(self, tag, attrs):
        for name, value in attrs:
            if name.lower() == "href" and value is not None:
                if tag.lower() == "base":
                    if self.base is None:
                        self.base = value.strip()
                else:
                    self.hrefs.append(value.strip())

    handle_startendtag = handle_starttag

p = LinkParser()
p.feed(resp["body"])
p.close()

base = urljoin(PAGE, p.base) if p.base else PAGE
links = set()
for href in p.hrefs:  # attribute values already have entities decoded by HTMLParser
    if href.lower().startswith("mailto:"):
        continue
    links.add(urljoin(base, href))

links = sorted(links)
print(len(links), links)
await submit_answer({"links": links})
