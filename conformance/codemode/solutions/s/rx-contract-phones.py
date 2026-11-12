import re

text = await read_file({"path": "/docs/contract.txt"})

# "+" then digits, spaces, parentheses, hyphens; must end with a digit (greedy, backtracks to last digit).
pattern = re.compile(r'\+[0-9 ()\-]*[0-9]')
phones = [m.group(0) for m in pattern.finditer(text)]

print(len(phones), phones)
await submit_answer({"phones": phones})
