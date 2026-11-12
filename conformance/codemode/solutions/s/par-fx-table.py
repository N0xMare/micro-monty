quotes = ["EUR", "GBP", "CAD", "JPY", "CHF", "AUD"]
rates = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": q}) for q in quotes))
converted = {q: round(2500 * r["rate"], 2) for q, r in zip(quotes, rates)}
print(converted)
await submit_answer({"converted": converted})
