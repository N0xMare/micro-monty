# `deque.remove(x)` whose `==` changes the deque raises CPython's
# `IndexError('deque mutated during iteration')`.
from collections import deque

dq = deque([1, 2, 3])


class Clears:
    def __eq__(self, o):
        dq.clear()
        return True


class Appends:
    def __eq__(self, o):
        dq.append(9)
        return False


for x in Clears(), Appends():
    dq.extend([1, 2, 3])
    try:
        dq.remove(x)
        print('removed', dq)
    except IndexError as e:
        print(type(e).__name__, e, len(dq))
