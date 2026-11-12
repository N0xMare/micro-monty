import json, statistics

rows = json.loads(await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"}))
totals = [float(r["total"]) for r in rows if r["total"] is not None]
result = {
    "mean": round(statistics.mean(totals), 2),
    "median": round(statistics.median(totals), 2),
    "stdev": round(statistics.stdev(totals), 2),
    "min": round(min(totals), 2),
    "max": round(max(totals), 2),
    "count": len(totals),
}
print(result)
print(await submit_answer(result))
