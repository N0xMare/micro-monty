result = await get_metrics({"host": "db-01", "metric": "mem"})
values = [value for _, value in result["points"]]
averages = []
for start in range(max(0, len(values) - 3)):
    window = values[start:start + 4]
    averages.append(round(sum(window) / 4, 2) if all(value is not None for value in window) else None)
await submit_answer({"averages": averages})
