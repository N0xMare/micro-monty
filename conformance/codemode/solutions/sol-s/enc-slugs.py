import re
import html
import xml.etree.ElementTree as ET

response = await http_get({"url": "https://news.example.com/feed.xml"})
root = ET.fromstring(response["body"])
slugs = []
for item in root.iter("item"):
    title = item.find("title")
    text = "" if title is None else "".join(title.itertext())
    slugs.append(re.sub(r"[^a-z0-9]+", "-", html.unescape(text).lower()).strip("-"))
await submit_answer({"slugs": slugs})
