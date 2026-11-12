import email.utils
import xml.etree.ElementTree as ET
from datetime import timezone
from html import unescape

response = await http_get({"url": "https://news.example.com/feed.xml"})
root = ET.fromstring(response["body"])
items = []
for item in root.findall(".//item"):
    title = unescape(item.findtext("title", ""))
    date = email.utils.parsedate_to_datetime(item.findtext("pubDate", ""))
    date = date.replace(tzinfo=timezone.utc) if date.tzinfo is None else date.astimezone(timezone.utc)
    items.append({"title": title, "published": date.strftime("%Y-%m-%dT%H:%M:%SZ")})
items.sort(key=lambda item: item["published"], reverse=True)
await submit_answer({"items": items})
