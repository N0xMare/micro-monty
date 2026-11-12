import re

text = await read_file({"path": "/docs/contract.txt"})

pattern = re.compile(r'[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}')
emails = sorted({m.group(0).strip('.') for m in pattern.finditer(text)})

print(len(emails), emails)
await submit_answer({"emails": emails})
