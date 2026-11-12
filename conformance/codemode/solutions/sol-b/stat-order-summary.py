import json
import statistics
rows = json.loads(await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"}))
values = [row["total"] for row in rows]
await submit_answer({"mean": round(statistics.mean(values), 2), "median": round(statistics.median(values), 2), "stdev": round(statistics.stdev(values), 2), "min": round(min(values), 2), "max": round(max(values), 2), "count": len(values)})
