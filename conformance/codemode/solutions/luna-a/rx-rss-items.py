import json
import xml.etree.ElementTree as ET
from datetime import timezone
from email.utils import parsedate_to_datetime
from html import unescape

xml = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))["body"]
root = ET.fromstring(xml)
items = []
for item in root.findall(".//item"):
    title = unescape(item.findtext("title", ""))
    dt = parsedate_to_datetime(item.findtext("pubDate", "")).astimezone(timezone.utc)
    items.append({"title": title, "published": dt.strftime("%Y-%m-%dT%H:%M:%SZ")})
items.sort(key=lambda x: x["published"], reverse=True)
await submit_answer({"items": items})
