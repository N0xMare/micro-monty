import json
from urllib.parse import quote

for term in ["tablet", "monitor", "hub"]:
    page = json.loads(await http_get({"url": f"https://shop.example.com/api/search?q={quote(term)}"}))
    results = page["results"]
    if results:
        cheapest = min(results, key=lambda r: r["price"])
        await submit_answer({"term": term, "product_id": cheapest["id"]})
        break
