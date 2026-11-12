from statistics import mean, pstdev

series = await get_metrics({"host": "web-05", "metric": "cpu"})
points = [(t, v) for t, v in series["points"] if v is not None]
values = [v for t, v in points]
center, deviation = mean(values), pstdev(values)
await submit_answer({"timestamps": sorted(t for t, v in points if abs(v - center) > 2 * deviation)})
