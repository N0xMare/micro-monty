import json
import xml.etree.ElementTree as ET
from urllib.parse import urlsplit, parse_qs
body = json.loads(await http_get({"url": "https://shop.example.com/sitemap.xml"}))["body"]
root = ET.fromstring(body)
ids = set()
for entry in root.findall("{*}url"):
    location = entry.findtext("{*}loc", "")
    modified = entry.findtext("{*}lastmod", "")
    parts = urlsplit(location)
    query = parse_qs(parts.query)
    if parts.path.rstrip("/") == "/products" and modified[:10] >= "2026-03-06" and "id" in query:
        ids.add(int(query["id"][0]))
await submit_answer({"ids": sorted(ids)})
