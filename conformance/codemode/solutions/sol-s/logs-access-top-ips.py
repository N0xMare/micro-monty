from collections import Counter

log = await get_access_log({"date": "2026-03-14"})
counts = Counter(line.split()[0] for line in log.splitlines() if line.strip())
top = [{"ip": ip, "requests": count} for ip, count in sorted(counts.items(), key=lambda x: (-x[1], x[0]))[:3]]
await submit_answer({"top": top})
