import re

text = await read_file({"path": "/docs/contract.txt"})
await submit_answer({"invoices": re.findall(r"\bINV-\d{4}-\d{4}\b", text)})
