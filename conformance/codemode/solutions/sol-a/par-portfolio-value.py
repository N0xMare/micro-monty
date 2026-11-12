import asyncio
import json
from decimal import Decimal

holdings = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
results = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in holdings))
values = {symbol: Decimal(str(json.loads(result, parse_float=Decimal)["price"])) * shares for (symbol, shares), result in zip(holdings.items(), results)}
largest = min(values, key=lambda symbol: (-values[symbol], symbol))
await submit_answer({"total_usd": float(round(sum(values.values(), Decimal(0)), 2)), "largest": largest})
