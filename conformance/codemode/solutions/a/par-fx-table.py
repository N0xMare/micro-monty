import json
import asyncio

quotes = ["EUR", "GBP", "CAD", "JPY", "CHF", "AUD"]
results = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": q}) for q in quotes))
converted = {}
for q, r in zip(quotes, results):
    d = json.loads(r)
    rate = d["rate"]
    if d.get("base") == q and d.get("quote") == "USD":
        rate = 1 / rate
    converted[q] = round(2500 * rate, 2)
print(converted)
await submit_answer({"converted": converted})
