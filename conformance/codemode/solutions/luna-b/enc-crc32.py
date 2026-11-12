import json, zlib

content = (await read_file({"path": "/release/report.txt"})).encode("utf-8")
value = zlib.crc32(content) & 0xffffffff
await submit_answer({"crc32": value, "hex": f"{value:08x}"})
