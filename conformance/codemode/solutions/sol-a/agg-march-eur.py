import asyncio
import json
from decimal import Decimal

rows_text, rate_text = await asyncio.gather(query_database({"sql": "SELECT COALESCE(SUM(total), 0) AS total FROM orders WHERE status = 'completed' AND order_date >= '2026-03-01' AND order_date < '2026-04-01'"}), get_exchange_rate({"base": "USD", "quote": "EUR"}))
total = Decimal(str(json.loads(rows_text)[0]["total"]))
rate = Decimal(str(json.loads(rate_text)["rate"]))
await submit_answer({"total_usd": float(round(total, 2)), "total_eur": float(round(total * rate, 2))})
