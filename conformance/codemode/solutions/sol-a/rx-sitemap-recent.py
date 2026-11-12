import json
import xml.etree.ElementTree as ET
from urllib.parse import urlsplit, parse_qs
body = json.loads(await http_get({"url": "https://shop.example.com/sitemap.xml"}))["body"]
root = ET.fromstring(body)
ids = set()
for entry in root.findall("{*}url"):
    loc = entry.findtext("{*}loc", "")
    lastmod = entry.findtext("{*}lastmod", "")
    parts = urlsplit(loc)
    if parts.path.rstrip("/") == "/products" and lastmod[:10] >= "2026-03-06":
        ids.update(int(value) for value in parse_qs(parts.query).get("id", []))
await submit_answer({"ids": sorted(ids)})
