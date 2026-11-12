import asyncio
from decimal import Decimal

currencies = ("EUR", "GBP", "CAD", "JPY", "CHF", "AUD")
rates = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": currency}) for currency in currencies))
await submit_answer({"converted": {currency: float(round(Decimal("2500") * Decimal(str(rate["rate"])), 2)) for currency, rate in zip(currencies, rates)}})
