import json
import re
import html
import xml.etree.ElementTree as ET

resp = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
print(resp["status"], resp["content_type"])
body = resp["body"]
try:
    root = ET.fromstring(body)
    titles = [(it.findtext("title") or "") for it in root.iter("item")]
    if not titles:
        titles = [(e.findtext("{http://www.w3.org/2005/Atom}title") or "")
                  for e in root.iter("{http://www.w3.org/2005/Atom}entry")]
    # XML parsing decodes one level; unescape any remaining HTML entities (e.g. &amp;eacute;)
    titles = [html.unescape(t) for t in titles]
except ET.ParseError:
    items = re.findall(r"<item\b.*?</item>", body, re.S)
    titles = []
    for it in items:
        m = re.search(r"<title[^>]*>(.*?)</title>", it, re.S)
        t = m.group(1) if m else ""
        cd = re.fullmatch(r"\s*<!\[CDATA\[(.*?)\]\]>\s*", t, re.S)
        t = cd.group(1) if cd else html.unescape(t)
        titles.append(html.unescape(t))
slugs = [re.sub(r"[^a-z0-9]+", "-", t.lower()).strip("-") for t in titles]
print(titles)
print(slugs)
print(await submit_answer({"slugs": slugs}))
