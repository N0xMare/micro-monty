import zlib
data = (await read_file({"path": "/release/report.txt"})).encode("utf-8")
checksum = zlib.crc32(data) & 0xffffffff
await submit_answer({"crc32": checksum, "hex": f"{checksum:08x}"})
