import json
import re
import html
from urllib.parse import urlsplit, parse_qs

resp = json.loads(await http_get({"url": "https://shop.example.com/sitemap.xml"}))
print("status", resp["status"])

ids = set()
for entry in re.findall(r"<url\b[^>]*>(.*?)</url>", resp["body"], re.S | re.I):
    loc = re.search(r"<loc>\s*(.*?)\s*</loc>", entry, re.S | re.I)
    lastmod = re.search(r"<lastmod>\s*(.*?)\s*</lastmod>", entry, re.S | re.I)
    if not loc or not lastmod:
        continue
    url = html.unescape(loc.group(1))
    qs = parse_qs(urlsplit(url).query)
    if "id" not in qs or not qs["id"][0].strip().isdigit():
        continue
    if lastmod.group(1)[:10] >= "2026-03-06":
        ids.add(int(qs["id"][0]))

ids = sorted(ids)
print(ids)
print(await submit_answer({"ids": ids}))
