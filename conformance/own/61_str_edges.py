s = "Hello World"
print(s.split("o"), s.rsplit(" ", 1) if False else 0, s.replace("l", "L"), s.replace("", "-") if False else 0)
print(s[2:-2], s[-3:-1], s[::-2], s[5:2], s[-1:-5:-1], s[1:100:3])
print("a\tb\nc".split(), " x ".split(" "), "a,b".split(",", 0) if False else 0, "abc".split("abc"))
print("x".join("123"), "|".join(["a"]), "abc".upper().lower(), "ß".upper() if False else 0)
print("%s" if False else 0, "abc".startswith(""), "".endswith(""), "abc".endswith(("c", "z")))
print("  a b  ".strip(" a"), "\n\t x\n".strip(), "abcabc".lstrip("ab"), "xyz".rstrip("z"))
print(repr(""), repr("'"), repr('"'), repr("\\"), repr("\r"), repr("\u00e9"), repr("\u200b"), repr("\x1b"))
print("é" * 3, len("日本語"), "日本語"[1], "日本語"[::-1], "a" < "é", "abc" in "xabcx")
print(str(3) + "x", "x" + str(3.5), "-" * 0, "ab" * -1, 2 * "ab")
print("{:>5}".format("x"), "{:.2f}".format(3.14159), "{!r}".format("s"), "{0[0]}".format([9]) if False else 0)
print(f"{'a' 'b'}", f"""{"triple"}""", f"{3}{4}", f"{'{'}", f"{{}}")
t = "abc"
for i in range(-4, 4):
    try:
        print(i, t[i])
    except IndexError as e:
        print(i, e)
print(list("ab"), tuple("ab"), sorted("cab"), min("cab"), max("cab"), "b" in ["a", "b"])
print(str(), repr(str()), ord("é"), chr(233), chr(0x1F600))
print('a中B'.title(), 'aǅB'.title())
for text in ('a', 'ab'):
    for width in (4, 5):
        print(repr(text.center(width, '-')))
