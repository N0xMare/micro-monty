import re

text = await read_file({"path": "/docs/contract.txt"})

# "+" followed by digits, spaces, parentheses and hyphens, ending with a digit.
pattern = re.compile(r"\+[0-9() -]*[0-9]")
phones = [m.group(0) for m in pattern.finditer(text)]

print(phones)
print(await submit_answer({"phones": phones}))
