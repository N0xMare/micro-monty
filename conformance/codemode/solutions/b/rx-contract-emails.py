import re

text = await read_file({"path": "/docs/contract.txt"})
found = re.findall(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}", text)
emails = sorted({e.strip(".") for e in found})
print(emails)
print(await submit_answer({"emails": emails}))
