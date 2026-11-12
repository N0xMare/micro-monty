import json
import re
from html.parser import HTMLParser
class Text(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.parts = []
    def handle_data(self, data):
        self.parts.append(data)
url = "https://shop.example.com/team"
result = json.loads(await http_get({"url": url}))
if result["status"] == 404:
    url = "https://shop.example.com/about"
    result = json.loads(await http_get({"url": url}))
parser = Text()
parser.feed(result["body"])
phone = re.search(r"\+[0-9](?:[0-9 ()-]*[0-9])?", " ".join(parser.parts)).group()
await submit_answer({"phone": phone, "source": url})
