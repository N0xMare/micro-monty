import asyncio
import re
from html.parser import HTMLParser
from html import unescape

class Page(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.text = []
        self.links = []
    def handle_data(self, data): self.text.append(data)
    def handle_starttag(self, tag, attrs):
        if tag.lower() == "a":
            href = dict(attrs).get("href", "")
            if href.lower().startswith("mailto:"):
                self.links.append(href[7:].split("?", 1)[0])

async def fetch(url):
    result = await http_get({"url": url})
    parser = Page()
    parser.feed(result["body"])
    return " ".join(parser.text), parser.links

pages = await asyncio.gather(fetch("https://shop.example.com/"), fetch("https://shop.example.com/about"))
emails = set()
for text, links in pages:
    emails.update(re.findall(r"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}", unescape(text), re.I))
    for href in links:
        emails.update(re.findall(r"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}", unescape(href), re.I))
await submit_answer({"emails": sorted(emails)})
