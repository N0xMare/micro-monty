import json

products = json.loads(await list_products({}))
prices = sorted(p["price"] for p in products)
n = len(prices)

def q(k):
    pos = (n - 1) * k / 4
    lo = int(pos)
    hi = min(lo + 1, n - 1)
    return prices[lo] + (prices[hi] - prices[lo]) * (pos - lo)

await submit_answer({"q1": round(q(1), 2), "q2": round(q(2), 2), "q3": round(q(3), 2)})
