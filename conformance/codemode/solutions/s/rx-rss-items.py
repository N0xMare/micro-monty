import re
import html
import datetime
from email.utils import parsedate_to_datetime

resp = await http_get({"url": "https://news.example.com/feed.xml"})
print(resp["status"], resp["content_type"])
body = resp["body"]

def text_of(tag, chunk):
    m = re.search(rf'<{tag}\b[^>]*>(.*?)</{tag}\s*>', chunk, re.S | re.I)
    if not m:
        return None
    s = m.group(1).strip()
    cdata = re.fullmatch(r'<!\[CDATA\[(.*?)\]\]>', s, re.S)
    if cdata:
        s = cdata.group(1)
    # Decode XML/HTML entities (repeatedly, to handle double-escaped titles like &amp;amp;)
    for _ in range(3):
        decoded = html.unescape(s)
        if decoded == s:
            break
        s = decoded
    return " ".join(s.split())

items = []
for chunk in re.findall(r'<item\b[^>]*>(.*?)</item\s*>', body, re.S | re.I):
    title = text_of("title", chunk)
    pub = text_of("pubDate", chunk)
    if pub is None:
        print("no pubDate:", title)
        continue
    try:
        dt = parsedate_to_datetime(pub)
    except (TypeError, ValueError):
        dt = datetime.datetime.fromisoformat(pub.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=datetime.timezone.utc)
    dt = dt.astimezone(datetime.timezone.utc)
    items.append((dt, {"title": title, "published": dt.strftime("%Y-%m-%dT%H:%M:%SZ")}))

items.sort(key=lambda x: x[0], reverse=True)
result = [it for _, it in items]
for it in result:
    print(it)
await submit_answer({"items": result})
