import re
from html import unescape

url = "https://shop.example.com/team"
response = await http_get({"url": url})
if response["status"] == 404:
    url = "https://shop.example.com/about"
    response = await http_get({"url": url})
if response["status"] != 200:
    raise RuntimeError(f"Page returned HTTP {response['status']}")
phone = re.search(r"\+[0-9](?:[0-9 ()-]*[0-9])?", unescape(response["body"]))[0]
await submit_answer({"phone": phone, "source": url})
