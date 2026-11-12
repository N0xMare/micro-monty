import re, html
from urllib.parse import urljoin, urlsplit, parse_qsl

page = "https://shop.example.com/"
resp = await http_get({"url": page})
print(resp["status"], resp["content_type"])
body = resp["body"]
links = []
for tag in re.findall(r"<a\b[^>]*>", body, re.I | re.S):
    m = re.search(r"""\bhref\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))""", tag, re.I)
    if not m:
        continue
    href = html.unescape(next(g for g in m.groups() if g is not None)).strip()
    full = urljoin(page, href)
    parts = urlsplit(full)
    if parts.scheme.lower() != "https":
        continue
    links.append({
        "host": parts.hostname or "",
        "path": parts.path or "/",
        "query": dict(parse_qsl(parts.query, keep_blank_values=True)),
    })
print(links)
await submit_answer({"links": links})
