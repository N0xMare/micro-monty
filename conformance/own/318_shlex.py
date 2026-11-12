# shlex: split, quote, join and the shlex lexer (docs/spec.md §4, §12).
import shlex
for s in ['ls -la "my file.txt" \'x y\' z\\ w', 'echo "a \\"quoted\\" word" # comment', 'a=1 b="2 3" cmd',
          '', '   ', "it's", 'a"b"c', 'echo $HOME "${X}"', 'x\\\\y', "'unterminated", '"also', 'trailing\\']:
    try:
        print(repr(s), shlex.split(s))
    except ValueError as e:
        print(repr(s), 'ValueError', e)
print(shlex.split('a # b c', comments=True), shlex.split('a "b c" d', posix=False))
for a in ['', 'safe-word_1.2/x:y@z', 'has space', "it's", 'tab\there', '$var', 'ünïcode', '*']:
    print(shlex.quote(a))
print(shlex.join(['git', 'commit', '-m', "fix: it's done", '--author=A B']))
args = ['a b', "c'd", '', 'e"f']
print(shlex.split(shlex.join(args)) == args)
lex = shlex.shlex('a && b || c; d > out.txt', punctuation_chars=True)
print(list(lex))
lex = shlex.shlex('key=value other="quoted thing"', posix=True)
lex.whitespace_split = True
print(list(lex))
lex = shlex.shlex('one two three')
print(lex.get_token(), lex.get_token()); lex.push_token('again'); print(lex.get_token(), lex.get_token(), repr(lex.get_token()))
lex = shlex.shlex('w1 w2', posix=True)
print(lex.error_leader(), lex.lineno, lex.wordchars[:10], lex.whitespace == ' \t\r\n')
for bad in [lambda: shlex.split(None), lambda: shlex.quote(5), lambda: shlex.quote(b'x')]:
    try:
        bad()
    except (ValueError, TypeError) as e:
        print(type(e).__name__, e)
print(shlex.split('multi\nline\targs'))
