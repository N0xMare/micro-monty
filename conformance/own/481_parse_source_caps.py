# ast.parse and tokenize take their whole source in one native step, so
# micro-monty caps it (32 KiB for ast.parse, 64 KiB for tokenize; past it,
# MemoryError). Sources exactly at the caps parse as CPython parses them.
import ast
import io
import tokenize

src = 'a\n' * 16_382 + 'é1\n'
assert len(src.encode()) == 32 * 1024
tree = ast.parse(src)
print(len(tree.body), type(tree.body[-1].value).__name__, tree.body[-1].value.id)
print(tree.body[-1].end_col_offset, tree.body[-1].lineno)

src = ('a;' * 31 + 'a\n') * 1_024
assert len(src.encode()) == 64 * 1024
toks = list(tokenize.generate_tokens(io.StringIO(src).readline))
print(len(toks), toks[-3], toks[-1])
print(sum(t.type == tokenize.OP for t in toks))
