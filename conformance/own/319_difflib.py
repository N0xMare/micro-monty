# difflib: unified_diff, context_diff, ndiff, restore, Differ, SequenceMatcher
# and get_close_matches (docs/spec.md §4, §12).
import difflib, sys
a = ['line one\n', 'line two\n', 'line three\n', 'line four\n', 'line five\n', 'six\n', 'seven\n', 'eight\n', 'nine\n']
b = ['line one\n', 'line 2\n', 'line three\n', 'line four\n', 'inserted\n', 'line five\n', 'six\n', 'seven\n', 'eight\n', 'nine!\n']
print("".join(difflib.unified_diff(a, b, fromfile='a.txt', tofile='b.txt')), end="")
print("".join(difflib.unified_diff(a, b, 'a', 'b', 'then', 'now', n=1)), end="")
print(list(difflib.unified_diff(['x'], ['y'], lineterm='')))
print(list(difflib.unified_diff([], [])), list(difflib.unified_diff(['a\n'], [])))
print("".join(difflib.context_diff(a, b, fromfile='a.txt', tofile='b.txt')), end="")
print("".join(difflib.context_diff(a, b, n=0)), end="")
print("".join(difflib.ndiff(['one\n', 'two\n', 'three\n'], ['ore\n', 'tree\n', 'emu\n'])), end="")
delta = list(difflib.ndiff(a, b))
print(''.join(difflib.restore(delta, 1)) == ''.join(a), ''.join(difflib.restore(delta, 2)) == ''.join(b))
s = difflib.SequenceMatcher(None, 'abxcd', 'abcd')
print(s.ratio(), s.quick_ratio(), s.real_quick_ratio(), s.get_matching_blocks(), s.get_opcodes())
s = difflib.SequenceMatcher(lambda x: x == ' ', 'private Thread currentThread;', 'private volatile Thread currentThread;')
print(round(s.ratio(), 3), s.find_longest_match(0, 29, 0, 38), s.find_longest_match())
s.set_seqs('qabxcd', 'abycdf'); print(s.get_opcodes(), list(s.get_grouped_opcodes(1)))
s = difflib.SequenceMatcher(None, 'a' * 300 + 'b', 'a' * 300 + 'c')
print(s.ratio(), len(s.bjunk), len(s.bpopular))
s = difflib.SequenceMatcher(None, 'a' * 300 + 'b', 'a' * 300 + 'c', autojunk=False)
print(round(s.ratio(), 4))
print(difflib.get_close_matches('appel', ['ape', 'apple', 'peach', 'puppy']))
print(difflib.get_close_matches('wheel', ['while', 'wheat', 'whale', 'wheels', 'heel'], n=2, cutoff=0.5))
print(difflib.get_close_matches('xyz', ['abc']))
for bad in [lambda: difflib.get_close_matches('a', ['b'], n=0), lambda: difflib.get_close_matches('a', ['b'], cutoff=2),
            lambda: list(difflib.unified_diff('abc', 'abd')), lambda: list(difflib.unified_diff([b'a'], [b'b'])),
            lambda: list(difflib.restore([], 3))]:
    try:
        bad()
    except (ValueError, TypeError) as e:
        print(type(e).__name__, e)
d = difflib.Differ()
print("".join(d.compare(['Hello world\n', 'keep\n'], ['Hallo world!\n', 'keep\n'])), end="")
print(difflib.IS_LINE_JUNK('  #\n'), difflib.IS_LINE_JUNK('x\n'), difflib.IS_CHARACTER_JUNK(' '))
print(difflib.Match(1, 2, 3), difflib.SequenceMatcher[int])
