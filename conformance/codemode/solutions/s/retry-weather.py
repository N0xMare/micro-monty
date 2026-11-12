import asyncio

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

weather = None
for attempt in range(4):  # 1 try + 3 retries
    try:
        weather = await get_weather({"city": "Madrid"})
        break
    except Exception as e:
        if not transient(e) or attempt == 3:
            raise
        print(f"attempt {attempt + 1} failed: {type(e).__name__}: {e}")
        await asyncio.sleep(0.5 * 2 ** attempt)

print(weather)
print(await submit_answer({"temp_c": weather["temp_c"]}))
