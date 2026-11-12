import asyncio
import json
import re
import html

def attrs(tag):
    return {k.lower(): html.unescape(v1 if v1 is not None else v2 if v2 is not None else v3)
            for k, v1, v2, v3 in re.findall(
                r"([\w:-]+)\s*=\s*(?:\"([^\"]*)\"|'([^']*)'|([^\s>\"']+))", tag)}

ids = range(1, 13)
results = await asyncio.gather(
    *(http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids),
    return_exceptions=True,
)

descriptions = {}
for i, r in zip(ids, results):
    if isinstance(r, Exception):
        print(i, "error", r)
        continue
    resp = json.loads(r)
    if resp["status"] != 200:
        print(i, "status", resp["status"])
        continue
    for tag in re.findall(r"<meta\b[^>]*>", resp["body"], re.I):
        a = attrs(tag)
        if a.get("name", "").lower() == "description" and "content" in a:
            descriptions[str(i)] = a["content"].strip()
            break
    else:
        print(i, "no description meta")

print(descriptions)
print(await submit_answer({"descriptions": descriptions}))
