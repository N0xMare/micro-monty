import asyncio
import html
import json
import re
from urllib.parse import unquote

urls = ["https://shop.example.com/", "https://shop.example.com/about"]
responses = await asyncio.gather(*(http_get({"url": u}) for u in urls))

email_re = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
emails = set()
for u, raw in zip(urls, responses):
    resp = json.loads(raw)
    if resp.get("status") != 200:
        print(u, "status", resp.get("status"))
        continue
    # Decode entities (e.g. &#64;) and percent-encoding in mailto: links before matching.
    text = unquote(html.unescape(resp["body"]))
    for m in email_re.finditer(text):
        emails.add(m.group(0).strip(".").lower())

emails = sorted(emails)
print(emails)
print(await submit_answer({"emails": emails}))
