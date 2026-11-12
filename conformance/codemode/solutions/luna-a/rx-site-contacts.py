import json
import re
from html.parser import HTMLParser

email_re = re.compile(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+")
class Contact(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.text = []
        self.hrefs = []
    def handle_data(self, data):
        self.text.append(data)
    def handle_starttag(self, tag, attrs):
        href = dict(attrs).get("href", "")
        if href.lower().startswith("mailto:"):
            self.hrefs.append(href[7:].split("?", 1)[0])

emails = set()
for url in ("https://shop.example.com/", "https://shop.example.com/about"):
    page = json.loads(await http_get({"url": url}))
    parser = Contact()
    parser.feed(page["body"])
    emails.update(email_re.findall(" ".join(parser.text)))
    for href in parser.hrefs:
        emails.update(email_re.findall(href))
await submit_answer({"emails": sorted(emails)})
