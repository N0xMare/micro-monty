# asyncio (as the unix standard variant freezes it, without its mip/ssl) plus the pure-Python
# micropython-lib modules a code-mode cell imports, frozen in so a session
# needs no filesystem (MPY_LIB_DIR = the micropython-lib checkout).
include("$(MPY_DIR)/extmod/asyncio")
for m in ["__future__", "abc", "base64", "bisect", "collections-defaultdict", "contextlib", "copy",
          "datetime", "fnmatch", "functools", "heapq", "hmac", "inspect", "itertools", "keyword",
          "logging", "operator", "pprint", "string", "textwrap", "types", "uuid", "warnings"]:
    require(m)
