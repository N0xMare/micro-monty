import asyncio
import json

shares = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
responses = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in shares))
quotes = {symbol: json.loads(response) for symbol, response in zip(shares, responses)}
values = {symbol: shares[symbol] * quote["price"] for symbol, quote in quotes.items()}
largest = min(values, key=lambda symbol: (-values[symbol], symbol))
await submit_answer({"total_usd": round(sum(values.values()), 2), "largest": largest})
