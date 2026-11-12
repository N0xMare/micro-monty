data = await get_metrics({"host": "db-01", "metric": "mem"})
values = [v for _, v in sorted(data["points"], key=lambda p: p[0])]

averages = []
for i in range(len(values) - 3):
    window = values[i:i + 4]
    averages.append(None if any(v is None for v in window) else round(sum(window) / 4, 2))

print(len(values), len(averages), averages)
await submit_answer({"averages": averages})
