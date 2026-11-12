import asyncio
import json
from decimal import Decimal

currencies = ("EUR", "GBP", "CAD", "JPY", "CHF", "AUD")
raw = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": currency}) for currency in currencies))
converted = {currency: float(round(Decimal("2500") * Decimal(str(json.loads(result)["rate"])), 2)) for currency, result in zip(currencies, raw)}
await submit_answer({"converted": converted})
