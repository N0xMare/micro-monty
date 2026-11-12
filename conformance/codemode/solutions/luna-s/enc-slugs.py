import html
import re
import xml.etree.ElementTree as ET

response = await http_get({"url": "https://news.example.com/feed.xml"})
root = ET.fromstring(response["body"])
slugs = []
for element in root.iter():
    if element.tag.rsplit("}", 1)[-1] in ("item", "entry"):
        for child in element:
            if child.tag.rsplit("}", 1)[-1] == "title":
                title = html.unescape("".join(child.itertext()))
                slug = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
                slugs.append(slug)
                break
await submit_answer({"slugs": slugs})
