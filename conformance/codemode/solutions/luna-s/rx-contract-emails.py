import re

text = await read_file({"path": "/docs/contract.txt"})
emails = sorted(set(re.findall(r"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}", text, re.I)))
await submit_answer({"emails": emails})
