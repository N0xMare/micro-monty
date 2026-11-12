from collections import Counter

text = await get_access_log({"date": "2026-03-14"})
counts = Counter(line.split()[0] for line in text.splitlines())
top = [{"ip": ip, "requests": count} for ip, count in sorted(counts.items(), key=lambda item: (-item[1], item[0]))[:3]]
await submit_answer({"top": top})
