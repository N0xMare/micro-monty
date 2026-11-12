text = "Grüße aus Köln – 東京 🚀"
data = text.encode("utf-8")
await submit_answer({"chars": len(text), "utf8_bytes": len(data), "utf16_bytes": len(text.encode("utf-16-le")), "hex": data.hex()})
