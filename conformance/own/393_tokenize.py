# tokenize over the own lexer: generate_tokens and tokenize (with its
# ENCODING token and coding cookies), comments, NL and NEWLINE, INDENT and
# DEDENT, f- and t-string pieces, exact_type, TokenInfo's repr, the token
# module, and TokenError, IndentationError and TabError. The first pattern
# is what models did in real cells: drop comments and strings, then look for
# keywords among the remaining NAME tokens.
import io
import token
import tokenize

SRC = '''# leading comment
def f(a, b=2):  # trailing
    """Doc."""
    try:
        raise ValueError(f"bad {a!r:>{b}} and {b:}")
    except ValueError:
        pass

    # indented comment
    return [a,  # inside brackets
            b] if a else t"x{a}"
x = 1 \\
    + 2; y = 0x1F + 1_000j + .5e-3
s = """multi
line""" + rb'raw' ...
'''


def code_text(src):
    out = []
    for tok in tokenize.generate_tokens(io.StringIO(src).readline):
        if tok.type in (tokenize.COMMENT, tokenize.STRING):
            continue
        if tok.type in (tokenize.NL, tokenize.NEWLINE):
            out.append('\n')
            continue
        if tok.type in (tokenize.INDENT, tokenize.DEDENT, tokenize.ENCODING,
                        tokenize.ENDMARKER):
            continue
        out.append(tok.string + ' ')
    return ''.join(out)


print(code_text(SRC))
kinds = sorted({t.string for t in tokenize.generate_tokens(io.StringIO(SRC).readline)
                if t.type == tokenize.NAME and t.string in ('raise', 'try', 'except')})
print(kinds)

for t in tokenize.generate_tokens(io.StringIO(SRC).readline):
    print(tokenize.tok_name[t.type], token.tok_name[t.exact_type], repr(t.string),
          t.start, t.end, repr(t.line))

toks = list(tokenize.generate_tokens(io.StringIO('x += 1').readline))
print(toks[1])
print(toks[1].exact_type, toks[1] == (tokenize.OP, '+=', (1, 2), (1, 4), 'x += 1'))
print(toks[-2:])
print(list(tokenize.generate_tokens(io.StringIO('').readline)))
print([(t.type, t.string) for t in tokenize.generate_tokens(io.StringIO('a ?? $b `c`\n').readline)])
print(list(tokenize.generate_tokens(io.StringIO('\n\n# c').readline)))
lines = iter(['a = 1\n', 'b\n'])
print(list(tokenize.generate_tokens(lambda: next(lines))))

# tokenize() reads bytes and starts with the encoding.
for t in tokenize.tokenize(io.BytesIO(b'# -*- coding: latin-1 -*-\nx = "\xe9"\n').readline):
    print(t)
for t in tokenize.tokenize(io.BytesIO('é = "ü"  # ñ\n'.encode()).readline):
    print(t)
print(tokenize.detect_encoding(io.BytesIO(b'\xef\xbb\xbfx = 1\n').readline))
print(tokenize.detect_encoding(io.BytesIO(b'#!/usr/bin/env python\n# vim: set fileencoding=utf_8 :\n').readline))

# Errors come after the tokens before them.
for bad in ["x = 'abc\n", 'x = (1,\n\n', '"""abc', 'x = 1_\n', 'a = 1\\',
            'if x:\n  y\n z\n', 'if x:\n\ty\n        z\n']:
    got = []
    try:
        for t in tokenize.generate_tokens(io.StringIO(bad).readline):
            got.append(t.string)
    except tokenize.TokenError as e:
        print(got, 'TokenError', e.args)
    except IndentationError as e:
        print(got, type(e).__name__, e.args)

print(token.ISTERMINAL(token.NAME), token.ISNONTERMINAL(token.NT_OFFSET),
      token.ISEOF(token.ENDMARKER), token.EXACT_TOKEN_TYPES['->'] == token.RARROW)
print(token.N_TOKENS, token.NT_OFFSET, token.tok_name[token.SOFT_KEYWORD])
print(len(token.__all__), token.__all__[:8], set(token.__all__) < set(tokenize.__all__))
print(issubclass(tokenize.TokenError, Exception), tokenize.TokenInfo._fields)
