# A 64 MiB power of two: CPython computes it; mm's pow preflight raises
# MemoryError under the default 256 MiB heap, though 1 << 2**29 succeeds.
print((2 ** (2 ** 29)).bit_length())
