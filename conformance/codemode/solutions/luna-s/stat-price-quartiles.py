products = await list_products({})
values = sorted(product["price"] for product in products)
def quartile(k):
    pos = (len(values) - 1) * k / 4
    lower = int(pos)
    upper = min(lower + 1, len(values) - 1)
    return values[lower] + (values[upper] - values[lower]) * (pos - lower)
await submit_answer({"q1": round(quartile(1), 2), "q2": round(quartile(2), 2), "q3": round(quartile(3), 2)})
