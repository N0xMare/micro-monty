import json
import math
points = json.loads(await get_metrics({"host": "web-03", "metric": "latency_ms"}))["points"]
values = sorted(value for t, value in points if value is not None)
await submit_answer({f"p{p}": values[math.ceil(p / 100 * len(values)) - 1] for p in (50, 90, 99)})
