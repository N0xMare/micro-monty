async def __codemode_main():
    global Counter, counts, line, log, m, pat, ranked, re, top, unparsed
    import re
    from collections import Counter

    log = await get_access_log({"date": "2026-03-14"})
    pat = re.compile(r'^(\S+) \S+ \S+ \[([^\]]*)\] "([^"]*)" (\d{3}) (\S+)')
    counts = Counter()
    unparsed = 0
    for line in log.splitlines():
        if not line.strip():
            continue
        m = pat.match(line)
        if not m:
            unparsed += 1
            continue
        counts[m.group(1)] += 1
    ranked = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
    top = [{"ip": ip, "requests": n} for ip, n in ranked[:3]]
    print(top, "unparsed:", unparsed)
    print(await submit_answer({"top": top}))
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
