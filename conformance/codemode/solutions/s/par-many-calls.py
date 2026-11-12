hosts = [h["host"] for h in await list_hosts({})]
calls = [(h, m) for h in hosts for m in ("errors", "latency_ms")]
series = await asyncio.gather(*(get_metrics({"host": h, "metric": m}) for h, m in calls))
data = {}
for (h, m), s in zip(calls, series):
    data[(h, m)] = [v for _, v in s["points"] if v is not None]
result = {}
for h in hosts:
    errs = data[(h, "errors")]
    lats = data[(h, "latency_ms")]
    result[h] = {
        "errors": int(round(sum(errs))),
        "max_latency_ms": float(max(lats)) if lats else None,
    }
print(result)
await submit_answer({"hosts": result})
