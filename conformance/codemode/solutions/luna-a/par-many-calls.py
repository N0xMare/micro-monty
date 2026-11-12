import asyncio
import json

hosts = json.loads(await list_hosts({}))
jobs = [(item["host"], metric) for item in hosts for metric in ("errors", "latency_ms")]
results = await asyncio.gather(*(get_metrics({"host": host, "metric": metric}) for host, metric in jobs))
series = {(host, metric): json.loads(result)["points"] for (host, metric), result in zip(jobs, results)}
summary = {}
for item in hosts:
    host = item["host"]
    error_values = [value for _, value in series[(host, "errors")] if value is not None]
    latency_values = [value for _, value in series[(host, "latency_ms")] if value is not None]
    summary[host] = {"errors": int(sum(error_values)), "max_latency_ms": max(latency_values) if latency_values else 0.0}
await submit_answer({"hosts": summary})
