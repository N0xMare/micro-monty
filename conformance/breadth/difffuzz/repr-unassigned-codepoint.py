# repr escaping: CPython escapes unassigned (Cn) and other
# non-printable code points in repr; mm prints them raw.
print(repr(chr(0x378)))
