text = await read_file({"path": "/docs/contract.txt"})
await submit_answer({
    "lines": text.count("\n"),
    "words": len(text.split()),
    "bytes": len(text.encode("utf-8")),
})
