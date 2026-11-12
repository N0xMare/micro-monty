import html
import json
import re
import xml.etree.ElementTree as ET

response = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
assert response["status"] == 200, response["status"]
root = ET.fromstring(response["body"])
slugs = []
for item in root.findall(".//item"):
    title = "".join(item.find("title").itertext())
    slugs.append(re.sub(r"[^a-z0-9]+", "-", html.unescape(title).lower()).strip("-"))
await submit_answer({"slugs": slugs})
