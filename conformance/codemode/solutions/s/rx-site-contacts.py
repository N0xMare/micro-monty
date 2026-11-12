import asyncio
import re
import html
from urllib.parse import unquote

urls = ["https://shop.example.com/", "https://shop.example.com/about"]
resps = await asyncio.gather(*[http_get({"url": u}) for u in urls])

email_re = re.compile(r'[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}')
FILE_EXT = {"png", "jpg", "jpeg", "gif", "svg", "webp", "css", "js", "ico", "avif"}

emails = set()
for u, r in zip(urls, resps):
    print(u, r["status"])
    body = html.unescape(r["body"])  # decodes &#64; / &commat; obfuscation and &amp;
    # mailto: links may be percent-encoded
    for m in re.finditer(r'mailto:([^"\'\s>?]+)', body, re.I):
        for e in email_re.findall(unquote(m.group(1))):
            emails.add(e)
    for e in email_re.findall(body):
        if e.rsplit(".", 1)[-1].lower() in FILE_EXT:
            continue
        emails.add(e)

emails = sorted(emails)
print(emails)
await submit_answer({"emails": emails})
