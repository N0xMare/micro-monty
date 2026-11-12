import re

text = await read_file({"path": "/docs/contract.txt"})
# "+" then digits, spaces, parentheses and hyphens, ending with a digit (no line breaks)
phones = [m.group(0) for m in re.finditer(r"\+[0-9 ()\-]*[0-9]", text)]
print(phones)
print(await submit_answer({"phones": phones}))
