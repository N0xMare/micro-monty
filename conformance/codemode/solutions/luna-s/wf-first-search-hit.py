import json
from urllib.parse import quote

term_found = None
product_id = None
for term in ("tablet", "monitor", "hub"):
    response = await http_get({"url": "https://shop.example.com/api/search?q=" + quote(term)})
    results = json.loads(response["body"])["results"]
    if results:
        term_found = term
        product_id = min(results, key=lambda item: item["price"])["id"]
        break
await submit_answer({"term": term_found or "", "product_id": product_id or ""})
