cities = ["London", "Atlantis", "Tokyo", "Gotham", "Austin", "Chicago"]
results = await asyncio.gather(*(get_weather({"city": c}) for c in cities), return_exceptions=True)
temps = {}
failed = []
for c, r in zip(cities, results):
    if isinstance(r, Exception):
        print(c, "error:", r)
        failed.append(c)
    else:
        temps[c] = r["temp_c"]
print(temps, failed)
await submit_answer({"temps": temps, "failed": failed})
