import json
from urllib.parse import quote

answer = None
for term in ["tablet", "monitor", "hub"]:
    resp = await http_get({"url": f"https://shop.example.com/api/search?q={quote(term)}"})
    if resp["status"] != 200:
        print(term, "status", resp["status"])
        continue
    data = json.loads(resp["body"]) if isinstance(resp["body"], str) else resp["body"]
    results = data.get("results") or []
    print(term, len(results))
    if results:
        cheapest = min(results, key=lambda r: r["price"])
        answer = {"term": term, "product_id": cheapest["id"]}
        break

if answer is None:
    raise RuntimeError("no term had results")
await submit_answer(answer)
