from collections import Counter

text = await get_access_log({"date": "2026-03-14"})
counts = Counter(line.split(" ", 1)[0] for line in text.splitlines() if line.strip())
ranked = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
top = [{"ip": ip, "requests": n} for ip, n in ranked[:3]]
print(top, "next:", ranked[3:5])
print(await submit_answer({"top": top}))
