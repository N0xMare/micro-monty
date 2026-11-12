import zlib
content = await read_file({"path": "/release/report.txt"})
value = zlib.crc32(content.encode("utf-8")) & 0xffffffff
await submit_answer({"crc32": value, "hex": f"{value:08x}"})
