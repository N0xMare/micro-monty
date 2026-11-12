import json
import xml.etree.ElementTree as ET
from urllib.parse import urlsplit, parse_qs

page = json.loads(await http_get({"url": "https://shop.example.com/sitemap.xml"}))
root = ET.fromstring(page["body"])
ids = set()
for node in root.iter():
    if node.tag.rsplit("}", 1)[-1] != "url": continue
    loc = next((c.text for c in node if c.tag.rsplit("}", 1)[-1] == "loc"), "")
    lastmod = next((c.text for c in node if c.tag.rsplit("}", 1)[-1] == "lastmod"), "")
    if lastmod and lastmod[:10] >= "2026-03-06":
        value = parse_qs(urlsplit(loc).query).get("id", [None])[0]
        if value is not None:
            try: ids.add(int(value))
            except ValueError: pass
await submit_answer({"ids": sorted(ids)})
