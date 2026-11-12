import asyncio

hosts = await list_hosts({})
jobs = [(host["host"], metric) for host in hosts for metric in ("errors", "latency_ms")]
results = await asyncio.gather(*(get_metrics({"host": host, "metric": metric}) for host, metric in jobs))
by_host = {}
for (host, metric), result in zip(jobs, results):
    values = [value for _, value in result["points"] if value is not None]
    by_host.setdefault(host, {})
    if metric == "errors":
        by_host[host]["errors"] = int(sum(values))
    else:
        by_host[host]["max_latency_ms"] = max(values) if values else 0.0
await submit_answer({"hosts": by_host})
