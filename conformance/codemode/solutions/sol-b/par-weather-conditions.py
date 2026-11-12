import asyncio, json
from collections import Counter

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"]
weather = [json.loads(s) for s in await asyncio.gather(*(get_weather({"city": c}) for c in cities))]
await submit_answer({"conditions": dict(Counter(w["condition"] for w in weather))})
