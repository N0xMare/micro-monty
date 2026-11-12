data = [1, 5, 12, 7, 30]
if (n := len(data)) > 3:
    print("long", n)
print([y for x in data if (y := x * 2) > 10])
print(y)
while (chunk := data.pop()) != 12:
    print("pop", chunk)
assert n == 5
assert True, "unused"
try:
    assert n == 4, f"n was {n}"
except AssertionError as e:
    print("AssertionError:", e)
try:
    assert False
except AssertionError as e:
    print("bare", repr(e), e.args)
assert 1 == 2, ("tuple", "message")
