# Same root cause, as users see it: int()/float() error messages embed repr(arg).
try:
    int(chr(0x1aec))
except ValueError as e:
    print(e)
