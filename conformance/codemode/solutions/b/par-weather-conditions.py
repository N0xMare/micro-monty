import asyncio, json

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney",
          "New York", "Chicago", "Austin", "Toronto"]
results = [json.loads(s) for s in await asyncio.gather(*(get_weather({"city": c}) for c in cities))]

conditions = {}
for w in results:
    conditions[w["condition"]] = conditions.get(w["condition"], 0) + 1

print(conditions)
print(await submit_answer({"conditions": conditions}))
