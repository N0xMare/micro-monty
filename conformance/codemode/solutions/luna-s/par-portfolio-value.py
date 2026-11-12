import asyncio

shares = {"ACME": 120, "GLBX": 300, "INIT": 15, "UMBR": 1000, "WAYN": 40}
symbols = list(shares)
quotes = await asyncio.gather(*(get_quote({"symbol": symbol}) for symbol in symbols))
values = {symbol: shares[symbol] * quote["price"] for symbol, quote in zip(symbols, quotes)}
await submit_answer({"total_usd": round(sum(values.values()), 2), "largest": max(values, key=values.get)})
