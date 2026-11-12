import json
from urllib.parse import quote

answer = None
for term in ["tablet", "monitor", "hub"]:
    resp = json.loads(await http_get({"url": f"https://shop.example.com/api/search?q={quote(term)}"}))
    if resp["status"] != 200:
        raise RuntimeError(f"search {term!r} returned status {resp['status']}")
    results = json.loads(resp["body"]).get("results") or []
    if results:
        cheapest = min(results, key=lambda r: r["price"])
        answer = {"term": term, "product_id": cheapest["id"]}
        break

if answer is None:
    raise ValueError("No term returned any results")
await submit_answer(answer)
