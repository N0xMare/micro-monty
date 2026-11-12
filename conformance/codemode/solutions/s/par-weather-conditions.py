import asyncio
from collections import Counter

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney",
          "New York", "Chicago", "Austin", "Toronto"]
results = await asyncio.gather(*(get_weather({"city": c}) for c in cities))

conditions = dict(Counter(r["condition"] for r in results))
print(conditions)
await submit_answer({"conditions": conditions})
