import json
import re
import html
from datetime import timezone
from email.utils import parsedate_to_datetime

resp = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
print("status", resp["status"])
body = resp["body"]

def field(item, tag):
    m = re.search(rf"<{tag}\b[^>]*>(.*?)</{tag}>", item, re.S | re.I)
    if not m:
        return None
    raw = m.group(1).strip()
    c = re.fullmatch(r"<!\[CDATA\[(.*?)\]\]>", raw, re.S)
    return (c.group(1) if c else raw).strip()

items = []
for item in re.findall(r"<item\b[^>]*>(.*?)</item>", body, re.S | re.I):
    title = field(item, "title")
    pub = field(item, "pubDate")
    title = re.sub(r"\s+", " ", html.unescape(title or "")).strip()
    dt = parsedate_to_datetime(html.unescape(pub)).astimezone(timezone.utc)
    items.append((dt, {"title": title, "published": dt.strftime("%Y-%m-%dT%H:%M:%SZ")}))

items.sort(key=lambda x: x[0], reverse=True)
result = [d for _, d in items]
for r in result:
    print(r)
print(await submit_answer({"items": result}))
