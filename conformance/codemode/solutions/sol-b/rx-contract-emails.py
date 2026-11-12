import re
text = await read_file({"path": "/docs/contract.txt"})
emails = sorted(set(re.findall(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?\.[A-Za-z]{2,}", text)))
await submit_answer({"emails": emails})
