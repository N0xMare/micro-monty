# heapq: CPython 3.14's Lib/heapq.py (PSF license, see THIRD_PARTY.md),
# docstrings dropped; merge rewritten without bound __next__.
__all__ = ['heappush', 'heappop', 'heapify', 'heapreplace', 'heappushpop',
           'heappush_max', 'heappop_max', 'heapify_max', 'heapreplace_max',
           'heappushpop_max', 'nlargest', 'nsmallest', 'merge']
def heappush(heap, item):
    heap.append(item)
    _siftdown(heap, 0, len(heap)-1)
def heappop(heap):
    lastelt = heap.pop()    # raises appropriate IndexError if heap is empty
    if heap:
        returnitem = heap[0]
        heap[0] = lastelt
        _siftup(heap, 0)
        return returnitem
    return lastelt
def heapreplace(heap, item):
    returnitem = heap[0]    # raises appropriate IndexError if heap is empty
    heap[0] = item
    _siftup(heap, 0)
    return returnitem
def heappushpop(heap, item):
    if heap and heap[0] < item:
        item, heap[0] = heap[0], item
        _siftup(heap, 0)
    return item
def heapify(x):
    n = len(x)
    for i in reversed(range(n//2)):
        _siftup(x, i)
def heappop_max(heap):
    lastelt = heap.pop()    # raises appropriate IndexError if heap is empty
    if heap:
        returnitem = heap[0]
        heap[0] = lastelt
        _siftup_max(heap, 0)
        return returnitem
    return lastelt
def heapreplace_max(heap, item):
    returnitem = heap[0]    # raises appropriate IndexError if heap is empty
    heap[0] = item
    _siftup_max(heap, 0)
    return returnitem
def heappush_max(heap, item):
    heap.append(item)
    _siftdown_max(heap, 0, len(heap)-1)
def heappushpop_max(heap, item):
    if heap and item < heap[0]:
        item, heap[0] = heap[0], item
        _siftup_max(heap, 0)
    return item
def heapify_max(x):
    n = len(x)
    for i in reversed(range(n//2)):
        _siftup_max(x, i)
def _siftdown(heap, startpos, pos):
    newitem = heap[pos]
    while pos > startpos:
        parentpos = (pos - 1) >> 1
        parent = heap[parentpos]
        if newitem < parent:
            heap[pos] = parent
            pos = parentpos
            continue
        break
    heap[pos] = newitem
def _siftup(heap, pos):
    endpos = len(heap)
    startpos = pos
    newitem = heap[pos]
    childpos = 2*pos + 1    # leftmost child position
    while childpos < endpos:
        rightpos = childpos + 1
        if rightpos < endpos and not heap[childpos] < heap[rightpos]:
            childpos = rightpos
        heap[pos] = heap[childpos]
        pos = childpos
        childpos = 2*pos + 1
    heap[pos] = newitem
    _siftdown(heap, startpos, pos)
def _siftdown_max(heap, startpos, pos):
    'Maxheap variant of _siftdown'
    newitem = heap[pos]
    while pos > startpos:
        parentpos = (pos - 1) >> 1
        parent = heap[parentpos]
        if parent < newitem:
            heap[pos] = parent
            pos = parentpos
            continue
        break
    heap[pos] = newitem
def _siftup_max(heap, pos):
    'Maxheap variant of _siftup'
    endpos = len(heap)
    startpos = pos
    newitem = heap[pos]
    childpos = 2*pos + 1    # leftmost child position
    while childpos < endpos:
        rightpos = childpos + 1
        if rightpos < endpos and not heap[rightpos] < heap[childpos]:
            childpos = rightpos
        heap[pos] = heap[childpos]
        pos = childpos
        childpos = 2*pos + 1
    heap[pos] = newitem
    _siftdown_max(heap, startpos, pos)
def merge(*iterables, key=None, reverse=False):
    h = []
    if reverse:
        _heapify = heapify_max
        _heappop = heappop_max
        _heapreplace = heapreplace_max
        direction = -1
    else:
        _heapify = heapify
        _heappop = heappop
        _heapreplace = heapreplace
        direction = 1
    for order, it in enumerate(map(iter, iterables)):
        for value in it:
            h.append([value if key is None else key(value), order * direction, value, it])
            break
    _heapify(h)
    while len(h) > 1:
        s = h[0]
        yield s[2]
        for value in s[3]:
            s[0] = value if key is None else key(value)
            s[2] = value
            _heapreplace(h, s)
            break
        else:
            _heappop(h)
    if h:
        s = h[0]
        yield s[2]
        yield from s[3]
def nsmallest(n, iterable, key=None):
    if n == 1:
        it = iter(iterable)
        sentinel = object()
        result = min(it, default=sentinel, key=key)
        return [] if result is sentinel else [result]
    try:
        size = len(iterable)
    except (TypeError, AttributeError):
        pass
    else:
        if n >= size:
            return sorted(iterable, key=key)[:n]
    if isinstance(n, int) and n <= 0:
        return []
    if key is None:
        it = iter(iterable)
        result = [(elem, i) for i, elem in zip(range(n), it)]
        if not result:
            return result
        heapify_max(result)
        top = result[0][0]
        order = n
        _heapreplace = heapreplace_max
        for elem in it:
            if elem < top:
                _heapreplace(result, (elem, order))
                top, _order = result[0]
                order += 1
        result.sort()
        return [elem for (elem, order) in result]
    it = iter(iterable)
    result = [(key(elem), i, elem) for i, elem in zip(range(n), it)]
    if not result:
        return result
    heapify_max(result)
    top = result[0][0]
    order = n
    _heapreplace = heapreplace_max
    for elem in it:
        k = key(elem)
        if k < top:
            _heapreplace(result, (k, order, elem))
            top, _order, _elem = result[0]
            order += 1
    result.sort()
    return [elem for (k, order, elem) in result]
def nlargest(n, iterable, key=None):
    if n == 1:
        it = iter(iterable)
        sentinel = object()
        result = max(it, default=sentinel, key=key)
        return [] if result is sentinel else [result]
    try:
        size = len(iterable)
    except (TypeError, AttributeError):
        pass
    else:
        if n >= size:
            return sorted(iterable, key=key, reverse=True)[:n]
    if isinstance(n, int) and n <= 0:
        return []
    if key is None:
        it = iter(iterable)
        result = [(elem, i) for i, elem in zip(range(0, -n, -1), it)]
        if not result:
            return result
        heapify(result)
        top = result[0][0]
        order = -n
        _heapreplace = heapreplace
        for elem in it:
            if top < elem:
                _heapreplace(result, (elem, order))
                top, _order = result[0]
                order -= 1
        result.sort(reverse=True)
        return [elem for (elem, order) in result]
    it = iter(iterable)
    result = [(key(elem), i, elem) for i, elem in zip(range(0, -n, -1), it)]
    if not result:
        return result
    heapify(result)
    top = result[0][0]
    order = -n
    _heapreplace = heapreplace
    for elem in it:
        k = key(elem)
        if top < k:
            _heapreplace(result, (k, order, elem))
            top, _order, _elem = result[0]
            order -= 1
    result.sort(reverse=True)
    return [elem for (k, order, elem) in result]
_heappop_max  = heappop_max
_heapreplace_max = heapreplace_max
_heappush_max = heappush_max
_heappushpop_max = heappushpop_max
_heapify_max = heapify_max
