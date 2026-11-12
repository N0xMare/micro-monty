import asyncio, json

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def with_retry(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            print(f"attempt {attempt + 1} failed: {type(e).__name__}: {e}")
            await asyncio.sleep(0.2 * 2 ** attempt)

weather = json.loads(await with_retry(get_weather, {"city": "Madrid"}))
print(weather)
print(await submit_answer({"temp_c": weather["temp_c"]}))
