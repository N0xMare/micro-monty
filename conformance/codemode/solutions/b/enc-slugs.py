import json, re, html

resp = json.loads(await http_get({"url": "https://news.example.com/feed.xml"}))
print(resp["status"], resp["content_type"])
xml = resp["body"]

slugs = []
for item in re.findall(r"<item\b[^>]*>(.*?)</item>", xml, re.S | re.I):
    m = re.search(r"<title\b[^>]*>(.*?)</title>", item, re.S | re.I)
    if not m:
        continue
    title = m.group(1).strip()
    cdata = re.fullmatch(r"<!\[CDATA\[(.*?)\]\]>", title, re.S)
    title = cdata.group(1) if cdata else html.unescape(title)
    if cdata:
        title = html.unescape(title)
    slug = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
    slugs.append(slug)

print(slugs)
print(await submit_answer({"slugs": slugs}))
