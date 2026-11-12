text = await read_file({"path": "/docs/contract.txt"})
data = text.encode("utf-8")
lines = data.count(b"\n")          # wc -l counts newline characters
words = len(data.split())          # whitespace-separated words, like wc -w
nbytes = len(data)                 # wc -c
print(f"{lines:>7} {words:>7} {nbytes:>7} /docs/contract.txt")
print(await submit_answer({"lines": lines, "words": words, "bytes": nbytes}))
