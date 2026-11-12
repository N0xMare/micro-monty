text = await read_file({"path": "/docs/contract.txt"})
data = text.encode("utf-8")
await submit_answer({"lines": data.count(b"\n"), "words": len(text.split()), "bytes": len(data)})
