import asyncio
import json

hosts = json.loads(await list_hosts({}))
jobs = [(host["host"], metric) for host in hosts for metric in ("errors", "latency_ms")]
raw = await asyncio.gather(*(get_metrics({"host": host, "metric": metric}) for host, metric in jobs))
result = {host["host"]: {} for host in hosts}
for (host, metric), response in zip(jobs, raw):
    values = [value for ts, value in json.loads(response)["points"] if value is not None]
    if metric == "errors":
        result[host]["errors"] = int(sum(values))
    else:
        result[host]["max_latency_ms"] = max(values)
await submit_answer({"hosts": result})
