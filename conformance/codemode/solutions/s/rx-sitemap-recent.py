import re
import html
from urllib.parse import urlsplit, parse_qs

CUTOFF = "2026-03-06"

def tag_text(tag, chunk):
    m = re.search(rf'<(?:\w+:)?{tag}\b[^>]*>(.*?)</(?:\w+:)?{tag}\s*>', chunk, re.S | re.I)
    if not m:
        return None
    s = m.group(1).strip()
    c = re.fullmatch(r'<!\[CDATA\[(.*?)\]\]>', s, re.S)
    return html.unescape(c.group(1) if c else s).strip()

async def collect(url, seen):
    if url in seen:
        return []
    seen.add(url)
    r = await http_get({"url": url})
    print(url, r["status"])
    body = r["body"]
    entries = []
    # sitemap index: recurse into child sitemaps
    for chunk in re.findall(r'<(?:\w+:)?sitemap\b[^>]*>(.*?)</(?:\w+:)?sitemap\s*>', body, re.S | re.I):
        loc = tag_text("loc", chunk)
        if loc:
            entries += await collect(loc, seen)
    for chunk in re.findall(r'<(?:\w+:)?url\b[^>]*>(.*?)</(?:\w+:)?url\s*>', body, re.S | re.I):
        entries.append((tag_text("loc", chunk), tag_text("lastmod", chunk)))
    return entries

entries = await collect("https://shop.example.com/sitemap.xml", set())

ids = set()
for loc, lastmod in entries:
    if not loc:
        continue
    parts = urlsplit(loc)
    if not parts.path.rstrip("/").endswith("/products"):
        continue
    id_vals = parse_qs(parts.query).get("id")
    if not id_vals or not id_vals[0].strip().isdigit():
        continue
    if not lastmod:
        print("no lastmod:", loc)
        continue
    if lastmod[:10] >= CUTOFF:
        ids.add(int(id_vals[0]))

ids = sorted(ids)
print(ids)
await submit_answer({"ids": ids})
