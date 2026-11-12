import asyncio, json

hosts = [h["host"] for h in json.loads(await list_hosts({}))]
pairs = [(h, m) for h in hosts for m in ("errors", "latency_ms")]
results = await asyncio.gather(*(get_metrics({"host": h, "metric": m}) for h, m in pairs))

out = {h: {} for h in hosts}
for (h, m), r in zip(pairs, results):
    vals = [v for _, v in json.loads(r)["points"] if v is not None]
    if m == "errors":
        out[h]["errors"] = int(round(sum(vals)))
    else:
        out[h]["max_latency_ms"] = max(vals) if vals else None
print(out)
print(await submit_answer({"hosts": out}))
