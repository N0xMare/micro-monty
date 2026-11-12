import zlib

content = await read_file({"path": "/release/report.txt"})
crc = zlib.crc32(content.encode("utf-8")) & 0xFFFFFFFF
print(crc, f"{crc:08x}")
print(await submit_answer({"crc32": crc, "hex": f"{crc:08x}"}))
