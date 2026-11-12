import json
import xml.etree.ElementTree as ET
from datetime import timezone
from email.utils import parsedate_to_datetime
from html import unescape

feed = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
root = ET.fromstring(feed["body"])
items = []
for item in root.findall(".//item"):
    title = unescape(item.findtext("title") or "")
    dt = parsedate_to_datetime(item.findtext("pubDate"))
    published = dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    items.append({"title": title, "published": published})
items.sort(key=lambda x: x["published"], reverse=True)
await submit_answer({"items": items})
