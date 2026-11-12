value = "Grüße aus Köln – 東京 🚀"
utf8 = value.encode("utf-8")
await submit_answer({"chars": len(value), "utf8_bytes": len(utf8), "utf16_bytes": len(value.encode("utf-16-le")), "hex": utf8.hex()})
