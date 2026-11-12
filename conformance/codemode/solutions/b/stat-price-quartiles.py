import json, statistics

products = json.loads(await list_products({}))
prices = sorted(float(p["price"]) for p in products)
q1, q2, q3 = statistics.quantiles(prices, n=4, method="inclusive")
result = {"q1": round(q1, 2), "q2": round(q2, 2), "q3": round(q3, 2)}
print(len(prices), result)
print(await submit_answer(result))
