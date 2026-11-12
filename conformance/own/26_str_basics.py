s = "Hello, Wörld!"
print(len(s), s[0], s[-1], s[7], s[-6:], s[:5], s[::2], s[::-1], s[100:], s[-100:3])
print("ab" * 3, 3 * "x", "a" + "b", "" * 5, "abc" < "abd", "Z" < "a", "abc" == "abc")
print("lo" in s, "xyz" in s, "" in s, s.upper(), s.lower())
print("  pad  ".strip(), "xxhixx".strip("x"), "  l".lstrip(), "r  ".rstrip() + "|")
print("a,b,,c".split(","), "a b  c".split(), "a-b-c".split("-", 1), "".split())
print("-".join(["x", "y", "z"]), "".join([]), ", ".join("abc"))
print(s.startswith("Hell"), s.endswith("!"), s.startswith(("x", "H")), "aaa".replace("a", "b", 2))
print(s.find("o"), s.find("q"), s.count("l"), "x".zfill(3), "ab".center(6, "*"))
print(repr("it's"), repr('say "hi"'), repr("both ' \""), repr("tab\tnl\n\\"), repr("\x00\x7f"), repr("ü€😀"))
print(str(1), str(1.5), str(None), str([1, "a"]), str("x"), repr(str))
print("abc"[1:2], "abc"[5:], ord("a"), chr(65), "Abc".isupper(), "123".isdigit())
for ch in "héllo":
    print(ch, end="|")
print()
try:
    "abc"[3]
except IndexError as e:
    print(e)
try:
    "a" * "b"
except TypeError as e:
    print(e)
print("{} and {}".format(1, "two"), "{0}{1}{0}".format("a", "b"), "{x}".format(x=5))
