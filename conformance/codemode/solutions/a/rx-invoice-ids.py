import re

text = await read_file({"path": "/docs/contract.txt"})

invoices = re.findall(r"\bINV-\d{4}-\d{4}\b", text)

print(invoices)
print(await submit_answer({"invoices": invoices}))
