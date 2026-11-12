import zlib

content = await read_file({"path": "/release/report.txt"})
crc = zlib.crc32(content.encode("utf-8")) & 0xFFFFFFFF
h = f"{crc:08x}"
print(len(content), crc, h)
await submit_answer({"crc32": crc, "hex": h})
