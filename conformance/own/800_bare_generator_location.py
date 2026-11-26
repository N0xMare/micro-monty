import ast

# The generator expression spans the call's parentheses, not its function.
source = 'sum \\\n    (x for x in xs)'
gen = ast.parse(source, mode='eval').body.args[0]
print(gen.lineno, gen.col_offset, gen.end_lineno, gen.end_col_offset)
print(ast.get_source_segment(source, gen))
