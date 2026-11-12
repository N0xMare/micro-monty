import asyncio
import json
import re
import html
from urllib.parse import unquote

urls = ["https://shop.example.com/", "https://shop.example.com/about"]
responses = await asyncio.gather(*(http_get({"url": u}) for u in urls))

email_re = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
emails = set()
for u, r in zip(urls, responses):
    resp = json.loads(r)
    print(u, resp["status"])
    body = html.unescape(resp["body"])
    for m in re.finditer(r"mailto:([^\"'>\s?]+)", body, re.I):
        for addr in unquote(m.group(1)).split(","):
            if email_re.fullmatch(addr.strip()):
                emails.add(addr.strip())
    for e in email_re.findall(body):
        emails.add(e.strip("."))

emails = sorted(emails)
print(emails)
print(await submit_answer({"emails": emails}))
