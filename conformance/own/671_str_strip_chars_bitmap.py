# str.strip, lstrip and rstrip with `chars`: a short `chars` searched in
# place, and the membership bitmap a longer `chars`, a long receiver or
# many byte-char pairs build a slice of `chars` at a time. ASCII and
# non-ASCII operands, astral chars, chars past the receiver's highest code
# point, a `chars` long enough to build over several steps (a park or
# snapshot lands inside), and receivers long enough to trim over several.


def ends(t):
    return (len(t), t[:3], t[-3:])


# Short operands.
assert 'abc'.strip('é') == 'abc'
assert 'ébcé'.strip('é') == 'bc'
assert 'xxabxx'.lstrip('x') == 'abxx' and 'xxabxx'.rstrip('x') == 'xxab'
assert '🙂b🙂'.strip('🙂') == 'b'
assert 'ébcé'.strip('é' * 17) == 'bc'
assert 'abc'.strip('é' * 17) == 'abc'
assert '🙂b🙂'.strip('🙂' * 9) == 'b'
assert 'ébcé'.strip('é' * 17 + '\U0010ffff') == 'bc'
assert '\U0010ffffaé\U0010ffff'.strip('\U0010ffff' * 9) == 'aé'
assert 'xyabxy'.strip('xy' * 16) == 'ab'
assert 'xyabxyz'.strip('xy' * 16 + 'z') == 'ab'
assert 'ab'.strip('') == 'ab' and ''.strip('ab' * 40) == ''
assert '  ab \t'.strip(None) == 'ab' and '  ab \t'.rstrip(None) == '  ab'


class S(str):
    pass


assert S('zzaz').strip(S('z' * 40)) == 'a'
for bad in (5, b'a', ['a']):
    try:
        'abc'.strip(bad)
        raise AssertionError('stripped')
    except TypeError as e:
        assert str(e) == 'strip arg must be None or str', e

# Many byte-char pairs: a 30-char ASCII `chars` over an 80 KB receiver.
s = 'abcde' * 16_000 + 'Z' + 'edcba' * 16_000
c = 'abcdefghijklmnopqrstuvwxyABCDE'
assert s.strip(c) == 'Z' and s.lstrip(c) == 'Z' + 'edcba' * 16_000
assert s.rstrip(c) == 'abcde' * 16_000 + 'Z'

# A long `chars` (about 2.2 MiB, built over several steps) on short and
# medium receivers: hits early and late, a miss, non-ASCII on both sides.
d = 'ab' * 600_000 + 'é𐐷' * 100_000 + ''.join(map(chr, range(0x1f600, 0x1f650))) + 'c'
assert ('c' * 200).strip(d) == ''
assert ('z' * 200).strip(d) == 'z' * 200
assert '🙂éx𐐷'.strip(d) == 'x'
assert ('é𐐷' * 30 + 'жж' + 'bа' * 30).strip(d) == 'жж' + 'bа' * 30
assert ('𐐷🙂a' * 50 + 'Ω').lstrip(d) == 'Ω'
assert ('Ω' + 'c🙂é' * 50).rstrip(d) == 'Ω'
del d

# Long receivers (past 4 MiB): trimmed over several steps through a
# bitmap, ASCII and not, and with a bitmap of 256 chars.
t = 'ab' * 1_100_000 + 'XY' + 'ba' * 1_100_000
assert ends(t.strip('ab')) == (2, 'XY', 'XY')
assert ends(t.lstrip('ba')) == (2_200_002, 'XYb', 'aba')
assert ends(t.rstrip('ab')) == (2_200_002, 'aba', 'bXY')
t = 'жé' * 600_000 + '!' + 'éж' * 600_000
assert t.strip('éж') == '!'
assert ends(t.rstrip('ж' + 'é' * 40)) == (1_200_001, 'жéж', 'жé!')
assert t.strip('é') == t
t = 'ж' * 2_100_000 + 'x' + 'ѐ' * 2_100_000
r = ''.join(map(chr, range(0x400, 0x500)))
assert t.strip(r) == 'x' and ends(t.lstrip(r)) == (2_100_001, 'xѐѐ', 'ѐѐѐ')
assert t.strip('ab' * 40) == t
del t, r
