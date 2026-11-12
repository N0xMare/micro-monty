import asyncio
import json
from decimal import Decimal

currencies = ["EUR", "GBP", "CAD", "JPY", "CHF", "AUD"]
results = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": currency}) for currency in currencies))
converted = {currency: float(round(Decimal(2500) * Decimal(str(json.loads(result, parse_float=Decimal)["rate"])), 2)) for currency, result in zip(currencies, results)}
await submit_answer({"converted": converted})
