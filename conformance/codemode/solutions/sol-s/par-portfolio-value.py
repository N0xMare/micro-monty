import asyncio
from decimal import Decimal

holdings = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
quotes = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in holdings))
values = {symbol: Decimal(str(quote["price"])) * holdings[symbol] for symbol, quote in zip(holdings, quotes)}
await submit_answer({"total_usd": float(round(sum(values.values()), 2)), "largest": max(values, key=values.get)})
