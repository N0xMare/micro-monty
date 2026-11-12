import json
from urllib.parse import quote
answer = None
for term in ("tablet", "monitor", "hub"):
    response = json.loads(await http_get({"url": f"https://shop.example.com/api/search?q={quote(term)}"}))
    results = json.loads(response["body"])["results"]
    if results:
        answer = {"term": term, "product_id": min(results, key=lambda product: product["price"])["id"]}
        break
if answer is None:
    raise ValueError("No search term had results")
await submit_answer(answer)
