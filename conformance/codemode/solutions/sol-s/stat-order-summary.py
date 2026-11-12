from statistics import mean, median, stdev

rows = await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"})
values = [row["total"] for row in rows]
await submit_answer({"count": len(values), "mean": round(mean(values), 2), "median": round(median(values), 2), "stdev": round(stdev(values), 2), "min": round(min(values), 2), "max": round(max(values), 2)})
