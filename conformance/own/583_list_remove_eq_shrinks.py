# `list.remove(x)` whose `==` changes the list: CPython deletes the slice
# `[i:i + 1]` at the index of the match, so a list the `==` emptied or
# shortened past that index loses nothing more and no error is raised.
def t(tag, f):
    try:
        print(tag, f())
    except Exception as e:
        print(tag, type(e).__name__, e)


class Clears:
    def __init__(self, L):
        self.L = L

    def __eq__(self, o):
        self.L.clear()
        return True


L = [1, 2, 3]
t('needle clears', lambda: (L.remove(Clears(L)), L))

L = [Clears(None), 2, 3]
L[0].L = L
t('item clears', lambda: (L.remove(5), L))

L = [1, 2, 3, 4]


class Pops:
    def __eq__(self, o):
        if o == 3:
            del L[2:]
            return True
        return False


t('match past the end', lambda: (L.remove(Pops()), L))

L = [1, 2, 3, 4]


class Shifts:
    def __eq__(self, o):
        if o == 2:
            del L[0]
            return True
        return False


# The item now at the match's index goes, as on CPython.
t('shifted', lambda: (L.remove(Shifts()), L))

L = [1, 2, 3]


class Misses:
    def __eq__(self, o):
        L.clear()
        return False


t('cleared, no match', lambda: (L.remove(Misses()), L))

L = [1, 2, 3]


class Grows:
    def __eq__(self, o):
        L.extend([7, 8])
        return o == 2


t('grown', lambda: (L.remove(Grows()), L))
L = [1, 2, 3]
t('index', lambda: (L.index(Clears(L)), L))
L = [1, 2, 3]
t('count', lambda: (L.count(Clears(L)), L))
