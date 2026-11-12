import json
from urllib.parse import urlencode
for term in ["tablet", "monitor", "hub"]:
    response = json.loads(await http_get({"url": "https://shop.example.com/api/search?" + urlencode({"q": term})}))
    results = json.loads(response["body"])["results"]
    if results:
        cheapest = min(results, key=lambda product: product["price"])
        await submit_answer({"term": term, "product_id": cheapest["id"]})
        break
else:
    raise RuntimeError("No search term had results")
