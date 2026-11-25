import ast

# The AST keeps annotations the compiler doesn't evaluate or walk. Check
# each operator and its position without a recursive walk of the result.
prefix = 'not ' * 499 + '+-~' * 499
node = ast.parse('x: ' + prefix + 'value').body[0].annotation
offset = 3
for text, count, ops in [('not ', 499, (ast.Not,)), ('+-~', 499, (ast.UAdd, ast.USub, ast.Invert))]:
    for _ in range(count):
        for op in ops:
            assert isinstance(node.op, op) and node.col_offset == offset
            node = node.operand
            offset += len(text) // len(ops)
print(node.id, node.col_offset)

# A starred target passes Store to its direct child, but not to that
# child's own operands. A starred expression passes Load instead.
print(ast.dump(ast.parse('head, *obj.items[0] = [*items]')))

x = 5
print(+-~--+~x, not not not +-~x)
if not not not (x < 3 and 1 / 0):
    print('odd not chain took the branch')
if not not x:
    print('even not chain took the branch')
