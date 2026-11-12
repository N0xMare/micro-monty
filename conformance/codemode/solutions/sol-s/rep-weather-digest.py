import asyncio

cities = sorted(["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"])
weather = await asyncio.gather(*(get_weather({"city": c}) for c in cities))
text = "\n".join(f"{w['city']}: {w['temp_c']:.1f}°C, {w['condition']}" for w in weather)
await post_message({"channel": "#general", "text": text})
