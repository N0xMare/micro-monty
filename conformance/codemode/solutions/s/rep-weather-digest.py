import asyncio

cities = sorted(["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney",
                 "New York", "Chicago", "Austin", "Toronto"])
results = await asyncio.gather(*(get_weather({"city": c}) for c in cities))

text = "\n".join(f"{c}: {r['temp_c']:.1f}°C, {r['condition']}" for c, r in zip(cities, results))
print(text)

res = await post_message({"channel": "#general", "text": text})
print(res)
