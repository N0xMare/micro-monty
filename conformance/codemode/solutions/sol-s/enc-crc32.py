import zlib

content = await read_file({"path": "/release/report.txt"})
checksum = zlib.crc32(content.encode("utf-8")) & 0xffffffff
await submit_answer({"crc32": checksum, "hex": f"{checksum:08x}"})
