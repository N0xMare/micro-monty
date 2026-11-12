import asyncio, json

cities = sorted(["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"])
weather = [json.loads(s) for s in await asyncio.gather(*(get_weather({"city": c}) for c in cities))]
lines = [f"{w['city']}: {w['temp_c']:.1f}°C, {w['condition']}" for w in weather]
await post_message({"channel": "#general", "text": "\n".join(lines)})
