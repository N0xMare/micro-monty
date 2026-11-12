from statistics import quantiles

products = await list_products({})
quartiles = quantiles([product["price"] for product in products], n=4, method="inclusive")
await submit_answer({f"q{i}": round(value, 2) for i, value in enumerate(quartiles, 1)})
