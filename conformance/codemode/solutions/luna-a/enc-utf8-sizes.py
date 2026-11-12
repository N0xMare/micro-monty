text = "Grüße aus Köln – 東京 🚀"
utf8 = text.encode("utf-8")
await submit_answer({"chars": len(text), "utf8_bytes": len(utf8), "utf16_bytes": len(text.encode("utf-16-le")), "hex": utf8.hex()})
