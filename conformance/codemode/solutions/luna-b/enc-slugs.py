import json, re, xml.etree.ElementTree as ET
from html import unescape

response = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
root = ET.fromstring(response["body"])
titles = [unescape("".join(el.itertext())) for el in root.iter() if el.tag.rsplit("}", 1)[-1] == "title"]
slugs = [re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") for title in titles]
await submit_answer({"slugs": slugs})
