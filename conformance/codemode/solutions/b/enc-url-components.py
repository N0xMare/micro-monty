import json, re
from urllib.parse import urljoin, urlsplit, parse_qsl

base = "https://shop.example.com/"
resp = json.loads(await http_get({"url": base}))
print(resp["status"], resp["content_type"])
page = resp["body"]

links = []
for m in re.finditer(r"""<a\b[^>]*?\bhref\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))""", page, re.I | re.S):
    href = next(g for g in m.groups() if g is not None).strip().replace("&amp;", "&")
    url = urljoin(base, href)
    parts = urlsplit(url)
    if parts.scheme != "https":
        continue
    links.append({
        "host": parts.hostname or "",
        "path": parts.path,
        "query": dict(parse_qsl(parts.query, keep_blank_values=True)),
    })

print(json.dumps(links, indent=1))
print(await submit_answer({"links": links}))
