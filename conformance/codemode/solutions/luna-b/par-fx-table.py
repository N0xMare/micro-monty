import asyncio
import json

quotes = ["EUR", "GBP", "CAD", "JPY", "CHF", "AUD"]
responses = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": quote}) for quote in quotes))
converted = {quote: round(2500 * json.loads(response)["rate"], 2) for quote, response in zip(quotes, responses)}
await submit_answer({"converted": converted})
