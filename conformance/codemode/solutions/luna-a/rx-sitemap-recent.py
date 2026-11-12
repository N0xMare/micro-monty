import json
import xml.etree.ElementTree as ET
from urllib.parse import urlsplit, parse_qs

xml = json.loads(await http_get({"url": "https://shop.example.com/sitemap.xml"}))["body"]
root = ET.fromstring(xml)
ids = set()
for entry in root:
    loc = entry.find("{*}loc")
    lastmod = entry.find("{*}lastmod")
    if loc is None or lastmod is None or lastmod.text[:10] < "2026-03-06":
        continue
    value = parse_qs(urlsplit(loc.text).query).get("id", [])
    if value and value[0].isdigit():
        ids.add(int(value[0]))
await submit_answer({"ids": sorted(ids)})
