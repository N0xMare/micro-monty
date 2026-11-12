import asyncio
import json
from decimal import Decimal

holdings = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
raw = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in holdings))
values = {symbol: Decimal(str(json.loads(result)["price"])) * shares for (symbol, shares), result in zip(holdings.items(), raw)}
await submit_answer({"total_usd": float(round(sum(values.values()), 2)), "largest": max(values, key=values.get)})
