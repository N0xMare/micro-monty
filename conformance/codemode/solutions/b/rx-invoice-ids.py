import re

text = await read_file({"path": "/docs/contract.txt"})
invoices = []
for inv in re.findall(r"\bINV-\d{4}-\d{4}\b", text):
    if inv not in invoices:
        invoices.append(inv)
print(invoices)
print(await submit_answer({"invoices": invoices}))
