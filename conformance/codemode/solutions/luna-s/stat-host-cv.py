import asyncio
import statistics

hosts = await list_hosts({})
names = sorted(host["host"] for host in hosts)
series = await asyncio.gather(*(get_metrics({"host": host, "metric": "cpu"}) for host in names))
cvs = {}
for host, result in zip(names, series):
    values = [v for _, v in result["points"] if v is not None]
    cvs[host] = statistics.stdev(values) / statistics.mean(values) if len(values) > 1 and statistics.mean(values) else 0.0
await submit_answer({"most_variable": max(names, key=lambda h: (cvs[h], h)), "least_variable": min(names, key=lambda h: (cvs[h], h))})
