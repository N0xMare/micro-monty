import json, re, html
import xml.etree.ElementTree as ET
response = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
root = ET.fromstring(response["body"])
slugs = []
for item in root.iter("item"):
    title = item.find("title")
    text = "".join(title.itertext()) if title is not None else ""
    slugs.append(re.sub(r"[^a-z0-9]+", "-", html.unescape(text).lower()).strip("-"))
await submit_answer({"slugs": slugs})
