import asyncio, json

quotes = ["EUR", "GBP", "CAD", "JPY", "CHF", "AUD"]
results = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": q}) for q in quotes))
converted = {q: round(2500 * json.loads(r)["rate"], 2) for q, r in zip(quotes, results)}
print(converted)
print(await submit_answer({"converted": converted}))
