import json
import re
import xml.etree.ElementTree as ET
from html import unescape
r = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
root = ET.fromstring(r["body"]); slugs = []
for item in root.iter():
    if item.tag.rsplit("}", 1)[-1] == "item":
        title = next((x.text or "" for x in item if x.tag.rsplit("}", 1)[-1] == "title"), "")
        slugs.append(re.sub(r"[^a-z0-9]+", "-", unescape(title).lower()).strip("-"))
await submit_answer({"slugs": slugs})
