async def __codemode_main():
    global MAX_ATTEMPTS, asyncio, attempt, attempts, delay, e, quote
    import asyncio

    MAX_ATTEMPTS = 5
    quote = None
    attempts = 0
    delay = 1
    for attempt in range(1, MAX_ATTEMPTS + 1):
        attempts = attempt
        try:
            quote = await get_quote({"symbol": "WAYN"})
            break
        except RuntimeError as e:
            if "429" not in str(e):
                raise
            print(f"attempt {attempt}: {e}")
            if attempt == MAX_ATTEMPTS:
                raise RuntimeError(f"WAYN quote still rate limited after {MAX_ATTEMPTS} attempts") from e
            await asyncio.sleep(delay)
            delay *= 2

    print(quote, "attempts:", attempts)
    print(await submit_answer({"price": quote["price"], "attempts": attempts}))
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
