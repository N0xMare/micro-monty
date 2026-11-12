import asyncio, json

hosts = json.loads(await list_hosts({}))
host_names = sorted(host["host"] for host in hosts)
series = await asyncio.gather(*(get_metrics({"host": host, "metric": "cpu"}) for host in host_names))
lines = ["HOST       MEAN    MAX"]
for host, raw in zip(host_names, series):
    values = [point[1] for point in json.loads(raw)["points"] if point[1] is not None]
    if values:
        lines.append(f"{host:<8} {sum(values) / len(values):>6.1f} {max(values):>6.1f}")
await write_file({"path": "/out/cpu.txt", "content": "\n".join(lines) + "\n"})
