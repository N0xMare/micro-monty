def f():
    try:
        return "try"
    finally:
        print("finally runs")

print(f())

def g():
    try:
        return "try"
    finally:
        return "finally wins"

print(g())

def h():
    for i in range(5):
        try:
            if i == 1:
                continue
            if i == 3:
                break
            print("body", i)
        finally:
            print("cleanup", i)
    return i

print(h())

def swallow():
    try:
        1 / 0
    finally:
        return "swallowed"

print(swallow())

def nested():
    try:
        try:
            raise ValueError("inner")
        finally:
            print("inner finally")
    except ValueError as e:
        print("caught", e)
    finally:
        print("outer finally")

nested()

def ret_in_except():
    try:
        raise KeyError("k")
    except KeyError:
        return "from except"
    finally:
        print("finally after except return")

print(ret_in_except())

def loop_break_finally():
    while True:
        try:
            break
        finally:
            print("break finally")
    return "done"

print(loop_break_finally())
