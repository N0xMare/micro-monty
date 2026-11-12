import json
from urllib.parse import quote_plus

for term in ["tablet", "monitor", "hub"]:
    result = json.loads(await http_get({"url": "https://shop.example.com/api/search?q=" + quote_plus(term)}))
    body = json.loads(result["body"])
    if body["results"]:
        cheapest = min(body["results"], key=lambda r: (r["price"], r["id"]))
        await submit_answer({"term": term, "product_id": cheapest["id"]})
        break
