import re

text = await read_file({"path": "/docs/contract.txt"})

invoices = []
for m in re.finditer(r'\bINV-\d{4}-\d{4}\b', text):
    if m.group(0) not in invoices:
        invoices.append(m.group(0))

print(len(invoices), invoices)
await submit_answer({"invoices": invoices})
