import asyncio, json
from collections import Counter

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney",
          "New York", "Chicago", "Austin", "Toronto"]
results = [json.loads(r) for r in await asyncio.gather(
    *(get_weather({"city": c}) for c in cities))]
conditions = dict(Counter(w["condition"] for w in results))
print(conditions)
print(await submit_answer({"conditions": conditions}))
