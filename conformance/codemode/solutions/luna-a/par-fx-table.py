import asyncio
import json

currencies = ["EUR", "GBP", "CAD", "JPY", "CHF", "AUD"]
results = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": currency}) for currency in currencies))
converted = {currency: round(2500 * json.loads(result)["rate"], 2) for currency, result in zip(currencies, results)}
await submit_answer({"converted": converted})
