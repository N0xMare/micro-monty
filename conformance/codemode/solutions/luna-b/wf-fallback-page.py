import json
import re

url = "https://shop.example.com/team"
page = json.loads(await http_get({"url": url}))
if page["status"] == 404:
    url = "https://shop.example.com/about"
    page = json.loads(await http_get({"url": url}))
phone = re.search(r"\+\d[\d ()-]*\d", page["body"]).group(0)
await submit_answer({"phone": phone, "source": url})
