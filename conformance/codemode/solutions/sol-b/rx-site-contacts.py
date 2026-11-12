import asyncio
import json
import re
import html
from urllib.parse import unquote
pages = await asyncio.gather(*(http_get({"url": url}) for url in ["https://shop.example.com/", "https://shop.example.com/about"]))
emails = set()
for raw in pages:
    body = unquote(html.unescape(json.loads(raw)["body"]))
    emails.update(re.findall(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?\.[A-Za-z]{2,}", body))
await submit_answer({"emails": sorted(emails)})
