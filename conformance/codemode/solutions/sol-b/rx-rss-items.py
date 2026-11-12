import json
import html
import xml.etree.ElementTree as ET
from email.utils import parsedate_to_datetime
from datetime import timezone
body = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))["body"]
items = []
for item in ET.fromstring(body).findall(".//item"):
    published = parsedate_to_datetime(item.findtext("pubDate")).astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    items.append({"title": html.unescape(item.findtext("title", "")), "published": published})
items.sort(key=lambda item: item["published"], reverse=True)
await submit_answer({"items": items})
