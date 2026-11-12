import asyncio
import json

hosts = json.loads(await list_hosts({}))
results = await asyncio.gather(*(get_metrics({"host": host["host"], "metric": "cpu"}) for host in hosts))
lines = ["HOST       MEAN    MAX"]
for host, result in sorted(zip(hosts, results), key=lambda pair: pair[0]["host"]):
    values = [point[1] for point in json.loads(result)["points"] if point[1] is not None]
    if values:
        lines.append(f"{host['host']:<8} {sum(values) / len(values):>6.1f} {max(values):>6.1f}")
await write_file({"path": "/out/cpu.txt", "content": "\n".join(lines) + "\n"})
