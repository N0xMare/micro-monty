import asyncio

hosts = await list_hosts({})
names = sorted(h["host"] for h in hosts)
series = await asyncio.gather(*(get_metrics({"host": n, "metric": "cpu"}) for n in names))

lines = ["HOST       MEAN    MAX"]
for name, s in zip(names, series):
    vals = [v for _, v in s["points"] if v is not None]
    if vals:
        mean = sum(vals) / len(vals)
        mx = max(vals)
        lines.append(f"{name:<8} {mean:>6.1f} {mx:>6.1f}")
    else:
        lines.append(f"{name:<8} {'':>6} {'':>6}")
content = "\n".join(lines) + "\n"
print(content)

res = await write_file({"path": "/out/cpu.txt", "content": content})
print(res)
