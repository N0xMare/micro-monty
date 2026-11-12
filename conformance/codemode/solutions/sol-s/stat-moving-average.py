from statistics import mean

series = await get_metrics({"host": "db-01", "metric": "mem"})
values = [v for t, v in sorted(series["points"])]
averages = []
for i in range(len(values) - 3):
    window = values[i:i + 4]
    averages.append(None if None in window else round(mean(window), 2))
await submit_answer({"averages": averages})
