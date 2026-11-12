import asyncio
from statistics import mean, stdev

hosts = await list_hosts({})
series = await asyncio.gather(*(get_metrics({"host": host["host"], "metric": "cpu"}) for host in hosts))
coefficients = {}
for host, result in zip(hosts, series):
    values = [v for t, v in result["points"] if v is not None]
    coefficients[host["host"]] = stdev(values) / mean(values)
await submit_answer({"most_variable": max(coefficients, key=coefficients.get), "least_variable": min(coefficients, key=coefficients.get)})
