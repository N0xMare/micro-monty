import json
import re
import html
from urllib.parse import urljoin

page_url = "https://shop.example.com/"
resp = json.loads(await http_get({"url": page_url}))
print("status", resp["status"], resp["content_type"])
body = resp["body"]

base = page_url
m = re.search(r"<base\b[^>]*\bhref\s*=\s*(?:\"([^\"]*)\"|'([^']*)'|([^\s>]+))", body, re.I)
if m:
    base = urljoin(page_url, html.unescape(next(g for g in m.groups() if g is not None)).strip())

# Ignore commented-out markup
scan = re.sub(r"<!--.*?-->", "", body, flags=re.S)
links = set()
for m in re.finditer(r"<(?!base\b)[a-zA-Z][^>]*?\shref\s*=\s*(?:\"([^\"]*)\"|'([^']*)'|([^\s>\"']+))", scan, re.I):
    raw = next(g for g in m.groups() if g is not None)
    href = html.unescape(raw).strip()
    if href.lower().startswith("mailto:"):
        continue
    links.add(urljoin(base, href))

links = sorted(links)
print(len(links), links)
print(await submit_answer({"links": links}))
