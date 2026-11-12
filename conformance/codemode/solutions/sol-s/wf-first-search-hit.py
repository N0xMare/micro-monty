import json
from urllib.parse import quote

answer = None
for term in ("tablet", "monitor", "hub"):
    response = await http_get({"url": "https://shop.example.com/api/search?q=" + quote(term)})
    results = json.loads(response["body"])["results"]
    if results:
        answer = {"term": term, "product_id": min(results, key=lambda product: product["price"])["id"]}
        break
if answer is None:
    raise ValueError("No search term returned results")
await submit_answer(answer)
