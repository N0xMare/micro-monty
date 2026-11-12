import xml.etree.ElementTree as ET
from urllib.parse import urlsplit, parse_qs

response = await http_get({"url": "https://shop.example.com/sitemap.xml"})
root = ET.fromstring(response["body"])
ids = set()
for entry in root:
    fields = {child.tag.rsplit("}", 1)[-1]: child.text for child in entry}
    location = urlsplit(fields.get("loc", ""))
    if location.path.rstrip("/") == "/products" and fields.get("lastmod", "")[:10] >= "2026-03-06":
        ids.update(int(value) for value in parse_qs(location.query).get("id", []))
await submit_answer({"ids": sorted(ids)})
