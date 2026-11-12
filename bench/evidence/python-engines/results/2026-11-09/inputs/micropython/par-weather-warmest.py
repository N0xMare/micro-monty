async def __codemode_main():
    global asyncio, cities, coldest, mean_temp, results, warmest
    import asyncio

    cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney"]
    results = await asyncio.gather(*(get_weather({"city": c}) for c in cities))

    warmest = max(results, key=lambda r: r["temp_c"])["city"]
    coldest = min(results, key=lambda r: r["temp_c"])["city"]
    mean_temp = round(sum(r["temp_c"] for r in results) / len(results), 1)
    print(warmest, coldest, mean_temp)
    await submit_answer({"warmest": warmest, "coldest": coldest, "mean_temp_c": mean_temp})
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
