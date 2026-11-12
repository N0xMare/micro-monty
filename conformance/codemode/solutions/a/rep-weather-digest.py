import asyncio, json

cities = sorted(["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney",
                 "New York", "Chicago", "Austin", "Toronto"])
results = [json.loads(r) for r in await asyncio.gather(*(get_weather({"city": c}) for c in cities))]
text = "\n".join(f"{c}: {w['temp_c']:.1f}°C, {w['condition']}" for c, w in zip(cities, results))
print(text)
print(await post_message({"channel": "#general", "text": text}))
