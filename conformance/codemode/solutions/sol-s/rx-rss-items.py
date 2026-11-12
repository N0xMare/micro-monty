import xml.etree.ElementTree as ET
from email.utils import parsedate_to_datetime
from datetime import timezone
from html import unescape

response = await http_get({"url": "https://news.example.com/feed.xml"})
root = ET.fromstring(response["body"])
items = []
for item in root.findall(".//item"):
    published = parsedate_to_datetime(item.findtext("pubDate")).astimezone(timezone.utc)
    items.append({"title": unescape(item.findtext("title", "")), "published": published.strftime("%Y-%m-%dT%H:%M:%SZ")})
items.sort(key=lambda item: item["published"], reverse=True)
await submit_answer({"items": items})
