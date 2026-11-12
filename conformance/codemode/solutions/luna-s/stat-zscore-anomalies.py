import statistics

result = await get_metrics({"host": "web-05", "metric": "cpu"})
points = [(timestamp, value) for timestamp, value in result["points"] if value is not None]
values = [value for _, value in points]
mean = statistics.mean(values)
sd = statistics.pstdev(values)
anomalies = [timestamp for timestamp, value in points if sd and abs(value - mean) > 2 * sd]
await submit_answer({"timestamps": anomalies})
