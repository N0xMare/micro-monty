import json
import re
import html
url = "https://shop.example.com/team"
response = json.loads(await http_get({"url": url}))
if response["status"] == 404:
    url = "https://shop.example.com/about"
    response = json.loads(await http_get({"url": url}))
phone = re.search(r"\+[0-9 ()-]*[0-9]", html.unescape(response["body"])).group()
await submit_answer({"phone": phone, "source": url})
