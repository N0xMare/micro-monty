import json, re

source = "https://shop.example.com/team"
resp = json.loads(await http_get({"url": source}))
if resp["status"] == 404:
    source = "https://shop.example.com/about"
    resp = json.loads(await http_get({"url": source}))

if resp["status"] != 200:
    raise RuntimeError(f"{source} returned status {resp['status']}")

body = resp["body"]
m = re.search(r"\+\d[\d\s().\-]{5,}\d", body)
if not m:
    raise ValueError(f"No phone number found on {source}: {body[:500]}")

await submit_answer({"phone": m.group(0).strip(), "source": source})
