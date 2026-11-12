import re

url = "https://shop.example.com/team"
response = await http_get({"url": url})
if response["status"] == 404:
    url = "https://shop.example.com/about"
    response = await http_get({"url": url})
match = re.search(r"\+\d(?:[\d ()-]*\d)?", response["body"])
await submit_answer({"phone": match.group() if match else "", "source": url})
