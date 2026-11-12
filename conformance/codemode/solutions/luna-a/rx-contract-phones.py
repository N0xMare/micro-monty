import re

text = await read_file({"path": "/docs/contract.txt"})
phones = re.findall(r"\+(?:\d[\d ()-]*\d|\d)", text)
await submit_answer({"phones": phones})
