import asyncio
from decimal import Decimal

rows, rate = await asyncio.gather(query_database({"sql": "SELECT COALESCE(SUM(o.total), 0) AS total FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status = 'completed' AND c.country = 'GB'"}), get_exchange_rate({"base": "USD", "quote": "GBP"}))
await submit_answer({"total_gbp": float(round(Decimal(str(rows[0]["total"])) * Decimal(str(rate["rate"])), 2))})
