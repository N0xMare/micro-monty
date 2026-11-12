import statistics

rows = await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"})
totals = [float(r["total"]) for r in rows if r["total"] is not None]

result = {
    "count": len(totals),
    "mean": round(statistics.mean(totals), 2),
    "median": round(statistics.median(totals), 2),
    "stdev": round(statistics.stdev(totals), 2) if len(totals) > 1 else 0.0,
    "min": round(min(totals), 2),
    "max": round(max(totals), 2),
}
print(result)
await submit_answer(result)
