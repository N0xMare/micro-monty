import re, html

resp = await http_get({"url": "https://news.example.com/feed.xml"})
xml = resp["body"]
print(resp["status"], len(xml))

def unescape_all(s):
    for _ in range(5):
        u = html.unescape(s)
        if u == s:
            break
        s = u
    return s

items = re.findall(r"<item\b[^>]*>(.*?)</item>", xml, re.S | re.I)
if not items:
    items = re.findall(r"<entry\b[^>]*>(.*?)</entry>", xml, re.S | re.I)
slugs = []
for it in items:
    m = re.search(r"<title\b[^>]*>(.*?)</title>", it, re.S | re.I)
    if not m:
        continue
    t = m.group(1).strip()
    cd = re.fullmatch(r"<!\[CDATA\[(.*?)\]\]>", t, re.S)
    if cd:
        t = cd.group(1)
    t = unescape_all(t).lower()
    slugs.append(re.sub(r"[^a-z0-9]+", "-", t).strip("-"))
print(slugs)
await submit_answer({"slugs": slugs})
