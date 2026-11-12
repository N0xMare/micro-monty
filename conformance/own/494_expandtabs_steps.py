# A wide tab is written a step at a time (docs/spec.md §5): a short str
# whose tabs widen it past a step goes the stepped way a long str does,
# and a bytes or bytearray tab's spaces go out a slice at a time, the
# column keeping how many went. Results must match CPython.


def sig(t):
    return len(t), t[:12], t[-12:], t.count(' ' if isinstance(t, str) else b' ')


for size in [3_000_000, 1_048_576, 1_048_577, 2_097_153]:
    print(size, sig('\t'.expandtabs(size)), sig(b'\t'.expandtabs(size)))
    print(sig('ab\tc\xe9\t\n\tz'.expandtabs(size)))
    print(sig(b'ab\tc\xe9\t\r\tz'.expandtabs(size)), sig(bytearray(b'x\t\ty').expandtabs(size)))
print(sig(('x' + '\t' * 9).expandtabs(300_000)), sig((b'x' + b'\t' * 9).expandtabs(300_000)))
print(sig(('\t' * 600).expandtabs(1_000)), sig(('\u20ac\t' * 70_000).expandtabs(9)))
print(repr('a\tb'.expandtabs(0)), repr(b'a\tb'.expandtabs(-1)), repr('\t\t'.expandtabs(3)))
