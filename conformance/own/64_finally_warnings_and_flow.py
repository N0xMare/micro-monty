def a():
    for i in range(3):
        try:
            pass
        finally:
            if i == 1:
                break
    return i
print(a())
def b():
    try:
        return "try"
    finally:
        for j in range(2):
            if j:
                break
        print("loop in finally is fine")
print(b())
def c():
    while True:
        try:
            continue
        finally:
            return "finally return wins over continue"
print(c())
