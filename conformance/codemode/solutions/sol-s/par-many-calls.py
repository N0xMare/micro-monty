import asyncio

hosts = await list_hosts({})
keys = [(host["host"], metric) for host in hosts for metric in ("errors", "latency_ms")]
results = await asyncio.gather(*(get_metrics({"host": host, "metric": metric}) for host, metric in keys))
summary = {host["host"]: {} for host in hosts}
for (host, metric), result in zip(keys, results):
    values = [value for ts, value in result["points"] if value is not None]
    if metric == "errors":
        summary[host]["errors"] = int(sum(values))
    else:
        summary[host]["max_latency_ms"] = max(values)
await submit_answer({"hosts": summary})
