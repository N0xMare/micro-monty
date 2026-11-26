# Parentheses in a literal f-string format spec do not count as nested brackets.
import ast

source = "f'{value:" + '(' * 256 + "}'"
spec = ast.parse(source, mode='eval').body.values[0].format_spec
print(len(spec.values), len(spec.values[0].value), set(spec.values[0].value))
