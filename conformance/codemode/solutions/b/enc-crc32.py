import zlib

content = await read_file({"path": "/release/report.txt"})
crc = zlib.crc32(content.encode("utf-8")) & 0xFFFFFFFF
answer = {"crc32": crc, "hex": f"{crc:08x}"}
print(len(content), answer)
print(await submit_answer(answer))
