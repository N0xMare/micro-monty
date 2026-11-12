import html
import json
import re
import xml.etree.ElementTree as ET
from datetime import timezone
from email.utils import parsedate_to_datetime

resp = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
body = resp["body"]


def to_iso(s):
    dt = parsedate_to_datetime(s.strip())
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


raw_items = []
try:
    root = ET.fromstring(body)
    for item in root.iter():
        if item.tag.rsplit("}", 1)[-1] != "item":
            continue
        fields = {c.tag.rsplit("}", 1)[-1]: (c.text or "") for c in item}
        raw_items.append((fields.get("title", ""), fields.get("pubDate", "")))
except ET.ParseError:
    # Fall back to regex if the feed is not well-formed XML (e.g. HTML entities like &nbsp;).
    for m in re.finditer(r"<item\b.*?</item>", body, re.S | re.I):
        chunk = m.group(0)
        t = re.search(r"<title[^>]*>(.*?)</title>", chunk, re.S | re.I)
        d = re.search(r"<pubDate[^>]*>(.*?)</pubDate>", chunk, re.S | re.I)
        title = t.group(1) if t else ""
        cdata = re.fullmatch(r"\s*<!\[CDATA\[(.*?)\]\]>\s*", title, re.S)
        title = cdata.group(1) if cdata else title
        raw_items.append((title, d.group(1) if d else ""))

items = []
for title, pub in raw_items:
    # ElementTree decodes XML entities once; unescape again for double-encoded HTML entities.
    title = " ".join(html.unescape(title).split())
    items.append({"title": title, "published": to_iso(pub)})

items.sort(key=lambda it: it["published"], reverse=True)
print(items)
print(await submit_answer({"items": items}))
