import json
import math
products = json.loads(await list_products({}))
prices = sorted(product["price"] for product in products)
quartiles = {}
for k in (1, 2, 3):
    position = (len(prices) - 1) * k / 4
    lo, hi = math.floor(position), math.ceil(position)
    quartiles[f"q{k}"] = round(prices[lo] + (prices[hi] - prices[lo]) * (position - lo), 2)
await submit_answer(quartiles)
