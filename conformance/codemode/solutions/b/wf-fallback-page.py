import json, re, html

source = "https://shop.example.com/team"
resp = json.loads(await http_get({"url": source}))
if resp["status"] == 404:
    source = "https://shop.example.com/about"
    resp = json.loads(await http_get({"url": source}))
print(source, resp["status"], resp["content_type"])
if resp["status"] != 200:
    raise RuntimeError(f"unexpected status {resp['status']} for {source}")

body = html.unescape(resp["body"])
text = re.sub(r"<[^>]+>", " ", body)
matches = re.findall(r"\+\d[\d\s().\-]*\d", text)
print("phone candidates:", matches)
if not matches:
    print(body[:2000])
    raise RuntimeError("no phone number found")
phone = matches[0].strip()
print(await submit_answer({"phone": phone, "source": source}))
