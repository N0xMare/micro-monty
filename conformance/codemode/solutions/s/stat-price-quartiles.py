products = await list_products({})
prices = sorted(float(p["price"]) for p in products)
n = len(prices)

def quantile(k):
    pos = (n - 1) * k / 4
    lo = int(pos)
    hi = min(lo + 1, n - 1)
    frac = pos - lo
    return prices[lo] + (prices[hi] - prices[lo]) * frac

q1, q2, q3 = (round(quantile(k), 2) for k in (1, 2, 3))
print(n, q1, q2, q3)
await submit_answer({"q1": q1, "q2": q2, "q3": q3})
