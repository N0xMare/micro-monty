import json
from urllib.parse import quote

answer = None
for term in ["tablet", "monitor", "hub"]:
    resp = json.loads(await http_get({"url": f"https://shop.example.com/api/search?q={quote(term)}"}))
    if resp["status"] != 200:
        print(term, "status", resp["status"])
        continue
    results = json.loads(resp["body"]).get("results") or []
    print(term, len(results))
    if results:
        cheapest = min(results, key=lambda r: r["price"])
        answer = {"term": term, "product_id": cheapest["id"]}
        break

print(answer)
if answer is None:
    raise RuntimeError("no term had any results")
print(await submit_answer(answer))
