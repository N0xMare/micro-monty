import re
import xml.etree.ElementTree as ET
from urllib.parse import urlsplit, parse_qs

response = await http_get({"url": "https://shop.example.com/sitemap.xml"})
root = ET.fromstring(response["body"])
ids = set()
for entry in root:
    fields = {child.tag.rsplit("}", 1)[-1]: (child.text or "").strip() for child in entry}
    if fields.get("lastmod", "")[:10] >= "2026-03-06":
        values = parse_qs(urlsplit(fields.get("loc", "")).query).get("id", [])
        for value in values:
            if value.isdigit(): ids.add(int(value))
await submit_answer({"ids": sorted(ids)})
