import asyncio
import json
from decimal import Decimal

rows_text, rate_text = await asyncio.gather(query_database({"sql": "SELECT COALESCE(SUM(o.total), 0) AS total FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status = 'completed' AND c.country = 'GB'"}), get_exchange_rate({"base": "USD", "quote": "GBP"}))
total = Decimal(str(json.loads(rows_text)[0]["total"]))
rate = Decimal(str(json.loads(rate_text)["rate"]))
await submit_answer({"total_gbp": float(round(total * rate, 2))})
