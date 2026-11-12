import asyncio

currencies = ["EUR", "GBP", "CAD", "JPY", "CHF", "AUD"]
rates = await asyncio.gather(*(get_exchange_rate({"base": "USD", "quote": currency}) for currency in currencies))
converted = {currency: round(2500 * result["rate"], 2) for currency, result in zip(currencies, rates)}
await submit_answer({"converted": converted})
