s = "Grüße aus Köln – 東京 🚀"
b = s.encode("utf-8")
res = {"chars": len(s), "utf8_bytes": len(b), "utf16_bytes": len(s.encode("utf-16-le")), "hex": b.hex()}
print(res)
await submit_answer(res)
