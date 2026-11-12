import html
import json
import re
from urllib.parse import urlsplit, parse_qs

resp = json.loads(await http_get({"url": "https://shop.example.com/sitemap.xml"}))
body = resp["body"]

ids = set()
for m in re.finditer(r"<url\b.*?</url>", body, re.S | re.I):
    chunk = m.group(0)
    loc = re.search(r"<loc>\s*(.*?)\s*</loc>", chunk, re.S | re.I)
    lastmod = re.search(r"<lastmod>\s*(.*?)\s*</lastmod>", chunk, re.S | re.I)
    if not loc or not lastmod:
        continue
    day = lastmod.group(1)[:10]
    if day < "2026-03-06":
        continue
    q = parse_qs(urlsplit(html.unescape(loc.group(1))).query)
    for v in q.get("id", []):
        if v.strip().isdigit():
            ids.add(int(v))

ids = sorted(ids)
print(ids)
print(await submit_answer({"ids": ids}))
