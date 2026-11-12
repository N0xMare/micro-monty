import asyncio
import json
import re
import html
from urllib.parse import unquote
responses = await asyncio.gather(*(http_get({"url": url}) for url in ["https://shop.example.com/", "https://shop.example.com/about"]))
emails = set()
for response in responses:
    text = unquote(html.unescape(json.loads(response)["body"]))
    emails.update(re.findall(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+", text))
await submit_answer({"emails": sorted(emails)})
