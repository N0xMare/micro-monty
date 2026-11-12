import json
import statistics

hosts = json.loads(await list_hosts())
values = {}
for entry in hosts:
    host = entry["host"]
    points = json.loads(await get_metrics({"host": host, "metric": "cpu"}))["points"]
    samples = [p[1] for p in points if p[1] is not None]
    mean = statistics.mean(samples) if samples else 0
    values[host] = statistics.stdev(samples) / mean if len(samples) > 1 and mean else 0.0
ordered = sorted(values, key=lambda h: (values[h], h))
await submit_answer({"least_variable": ordered[0], "most_variable": ordered[-1]})
