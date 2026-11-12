import re

source = "https://shop.example.com/team"
resp = await http_get({"url": source})
if resp["status"] == 404:
    source = "https://shop.example.com/about"
    resp = await http_get({"url": source})
if resp["status"] != 200:
    raise RuntimeError(f"{source} returned {resp['status']}")

body = resp["body"]
m = re.search(r"\+\d[\d\s().\-]*\d", body)
if not m:
    print(body[:3000])
    raise RuntimeError("no phone number found")
phone = m.group(0).strip()
print(phone, source)
await submit_answer({"phone": phone, "source": source})
