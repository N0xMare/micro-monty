# ast: CPython 3.14's node classes and helpers over micro-monty's own
# parser. `_parse(source, mode, filename, module)` (native, astmod.rs)
# builds the tree from the classes below, which `_define` binds on the
# module (globals() is read-only, docs/spec.md §4). Left out: compile() of a tree,
# type comments, feature_version, optimize, and the deprecated aliases.

__all__ = ['AST', 'NodeTransformer', 'NodeVisitor', 'copy_location', 'dump',
           'fix_missing_locations', 'get_docstring', 'get_source_segment',
           'increment_lineno', 'iter_child_nodes', 'iter_fields', 'literal_eval',
           'parse', 'walk', 'PyCF_ONLY_AST']

PyCF_ONLY_AST = 1024
PyCF_TYPE_COMMENTS = 4096
PyCF_ALLOW_TOP_LEVEL_AWAIT = 8192
PyCF_OPTIMIZED_AST = 33792


class AST:
    _fields = ()
    _attributes = ()
    _lists = ()

    def __init__(self, *args, **kwargs):
        cls = type(self)
        fields = cls._fields
        if len(args) > len(fields):
            n = len(fields)
            raise TypeError(f"{cls.__name__} constructor takes at most "
                            f"{n} positional argument{'' if n == 1 else 's'}")
        for name, value in zip(fields, args):
            setattr(self, name, value)
        for name, value in kwargs.items():
            if name in fields[:len(args)]:
                raise TypeError(f"{cls.__name__} got multiple values for argument {name!r}")
            if name not in fields and name not in cls._attributes:
                _warn(f"{cls.__name__}.__init__ got an unexpected keyword argument "
                      f"{name!r}. Support for arbitrary keyword arguments is "
                      "deprecated and will be removed in Python 3.15.")
            setattr(self, name, value)
        for name in fields[len(args):]:
            if name in kwargs:
                continue
            if name in cls._lists:
                setattr(self, name, [])
            elif name == 'ctx':
                setattr(self, name, Load())
            elif name not in cls._optional:
                _warn(f"{cls.__name__}.__init__ missing 1 required positional "
                      f"argument: {name!r}. This will become an error in Python 3.15.")

    def __repr__(self):
        return _repr(self, 0)

    def __reduce__(self):
        return type(self), (), self.__dict__


def _warn(msg):
    import warnings
    warnings.warn(msg, DeprecationWarning, stacklevel=3)


def _repr(node, depth):
    # Python-ast.c's ast_repr_max_depth: nodes 3 levels down show as
    # `Name(...)`, lists longer than two as `[first, ..., last]`.
    name = type(node).__name__
    if depth >= 3:
        return name + '(...)'
    parts = []
    for f in node._fields:
        try:
            v = getattr(node, f)
        except AttributeError:
            continue
        parts.append(f'{f}={_repr_value(v, depth)}')
    return f"{name}({', '.join(parts)})"


def _repr_value(v, depth):
    if isinstance(v, AST):
        return _repr(v, depth + 1)
    if isinstance(v, list):
        if len(v) > 2:
            return f'[{_repr_value(v[0], depth)}, ..., {_repr_value(v[-1], depth)}]'
        return '[' + ', '.join(_repr_value(x, depth) for x in v) + ']'
    return repr(v)


# One base per line (`@` when its nodes have positions), then its
# subclasses; a field ending in `*` is a list, in `?` optional.
_SPEC = '''
mod
 Module body* type_ignores*
 Interactive body*
 Expression body
 FunctionType argtypes* returns
stmt @
 FunctionDef name args body* decorator_list* returns? type_comment? type_params*
 AsyncFunctionDef name args body* decorator_list* returns? type_comment? type_params*
 ClassDef name bases* keywords* body* decorator_list* type_params*
 Return value?
 Delete targets*
 Assign targets* value type_comment?
 TypeAlias name type_params* value
 AugAssign target op value
 AnnAssign target annotation value? simple
 For target iter body* orelse* type_comment?
 AsyncFor target iter body* orelse* type_comment?
 While test body* orelse*
 If test body* orelse*
 With items* body* type_comment?
 AsyncWith items* body* type_comment?
 Match subject cases*
 Raise exc? cause?
 Try body* handlers* orelse* finalbody*
 TryStar body* handlers* orelse* finalbody*
 Assert test msg?
 Import names*
 ImportFrom module? names* level?
 Global names*
 Nonlocal names*
 Expr value
 Pass
 Break
 Continue
expr @
 BoolOp op values*
 NamedExpr target value
 BinOp left op right
 UnaryOp op operand
 Lambda args body
 IfExp test body orelse
 Dict keys* values*
 Set elts*
 ListComp elt generators*
 SetComp elt generators*
 DictComp key value generators*
 GeneratorExp elt generators*
 Await value
 Yield value?
 YieldFrom value
 Compare left ops* comparators*
 Call func args* keywords*
 FormattedValue value conversion format_spec?
 Interpolation value str conversion format_spec?
 JoinedStr values*
 TemplateStr values*
 Constant value kind?
 Attribute value attr ctx?
 Subscript value slice ctx?
 Starred value ctx?
 Name id ctx?
 List elts* ctx?
 Tuple elts* ctx?
 Slice lower? upper? step?
expr_context
 Load
 Store
 Del
boolop
 And
 Or
operator
 Add
 Sub
 Mult
 MatMult
 Div
 Mod
 Pow
 LShift
 RShift
 BitOr
 BitXor
 BitAnd
 FloorDiv
unaryop
 Invert
 Not
 UAdd
 USub
cmpop
 Eq
 NotEq
 Lt
 LtE
 Gt
 GtE
 Is
 IsNot
 In
 NotIn
excepthandler @
 ExceptHandler type? name? body*
pattern @
 MatchValue value
 MatchSingleton value
 MatchSequence patterns*
 MatchMapping keys* patterns* rest?
 MatchClass cls patterns* kwd_attrs* kwd_patterns*
 MatchStar name?
 MatchAs pattern? name?
 MatchOr patterns*
type_ignore
 TypeIgnore lineno tag
type_param @
 TypeVar name bound? default_value?
 ParamSpec name default_value?
 TypeVarTuple name default_value?
AST
 comprehension target iter ifs* is_async
 arguments posonlyargs* args* vararg? kwonlyargs* kw_defaults* kwarg? defaults*
 arg@ arg annotation? type_comment?
 keyword@ arg? value
 alias@ name asname?
 withitem context_expr optional_vars?
 match_case pattern guard? body*
'''

_POSITIONS = ('lineno', 'col_offset', 'end_lineno', 'end_col_offset')
# The shared instances parsed trees use for contexts and operators.
_one = {}


def _define():
    import ast as g
    base = AST
    attrs = ()
    for line in _SPEC.split('\n'):
        if not line:
            continue
        words = line.split()
        name = words[0]
        if not line.startswith(' '):
            attrs = _POSITIONS if '@' in words else ()
            if name != 'AST':
                base = type(name, (AST,), {'__module__': 'ast', '_attributes': attrs,
                                           '_optional': ()})
                setattr(g, name, base)
            else:
                base = AST
            if attrs:
                base.end_lineno = base.end_col_offset = None
            continue
        own = attrs
        if name.endswith('@'):
            name = name[:-1]
            own = _POSITIONS
        fields = tuple(w.rstrip('*?') for w in words[1:])
        ns = {'__module__': 'ast', '_fields': fields, '__match_args__': fields,
              '_attributes': own,
              '_lists': tuple(w[:-1] for w in words[1:] if w[-1] == '*'),
              '_optional': tuple(w[:-1] for w in words[1:] if w[-1] == '?')}
        for f in ns['_optional']:
            ns[f] = None
        if own and not attrs:
            ns['end_lineno'] = ns['end_col_offset'] = None
        cls = type(name, (base,), ns)
        setattr(g, name, cls)
        if base.__name__ in ('expr_context', 'boolop', 'operator', 'unaryop', 'cmpop'):
            _one[name] = cls()


AST._optional = ()
_define()
import ast as _self


def parse(source, filename='<unknown>', mode='exec', *,
          type_comments=False, feature_version=None, optimize=-1):
    """Parse the source into an AST node."""
    if isinstance(source, (bytes, bytearray)):
        source = source.decode()
    if isinstance(filename, (bytes, bytearray)):
        filename = filename.decode()
    elif not isinstance(filename, str):
        filename = str(filename)
    return _parse(source, mode, filename, _self)


def literal_eval(node_or_string):
    """Evaluate an expression node or a string containing only a Python
    literal structure."""
    if isinstance(node_or_string, str):
        node_or_string = parse(node_or_string.lstrip(" \t"), mode='eval')
    if isinstance(node_or_string, Expression):
        node_or_string = node_or_string.body

    def _raise_malformed_node(node):
        msg = "malformed node or string"
        if lno := getattr(node, 'lineno', None):
            msg += f' on line {lno}'
        raise ValueError(msg + f': {node!r}')

    def _convert_num(node):
        if not isinstance(node, Constant) or type(node.value) not in (int, float, complex):
            _raise_malformed_node(node)
        return node.value

    def _convert_signed_num(node):
        if isinstance(node, UnaryOp) and isinstance(node.op, (UAdd, USub)):
            operand = _convert_num(node.operand)
            if isinstance(node.op, UAdd):
                return + operand
            else:
                return - operand
        return _convert_num(node)

    def _convert(node):
        if isinstance(node, Constant):
            return node.value
        elif isinstance(node, Tuple):
            return tuple(map(_convert, node.elts))
        elif isinstance(node, List):
            return list(map(_convert, node.elts))
        elif isinstance(node, Set):
            return set(map(_convert, node.elts))
        elif (isinstance(node, Call) and isinstance(node.func, Name) and
              node.func.id == 'set' and node.args == node.keywords == []):
            return set()
        elif isinstance(node, Dict):
            if len(node.keys) != len(node.values):
                _raise_malformed_node(node)
            return dict(zip(map(_convert, node.keys),
                            map(_convert, node.values)))
        elif isinstance(node, BinOp) and isinstance(node.op, (Add, Sub)):
            left = _convert_signed_num(node.left)
            right = _convert_num(node.right)
            if isinstance(left, (int, float)) and isinstance(right, complex):
                if isinstance(node.op, Add):
                    return left + right
                else:
                    return left - right
        return _convert_signed_num(node)
    return _convert(node_or_string)


def dump(node, annotate_fields=True, include_attributes=False, *,
         indent=None, show_empty=False):
    """Return a formatted dump of the tree in node."""
    def _format(node, level=0):
        if indent is not None:
            level += 1
            prefix = '\n' + indent * level
            sep = ',\n' + indent * level
        else:
            prefix = ''
            sep = ', '
        if isinstance(node, AST):
            cls = type(node)
            args = []
            args_buffer = []
            allsimple = True
            keywords = annotate_fields
            for name in node._fields:
                try:
                    value = getattr(node, name)
                except AttributeError:
                    keywords = True
                    continue
                if value is None and getattr(cls, name, ...) is None:
                    keywords = True
                    continue
                if not show_empty:
                    if value == [] and name in cls._lists:
                        if not keywords:
                            args_buffer.append(repr(value))
                        continue
                    if not keywords:
                        args.extend(args_buffer)
                        args_buffer = []
                value, simple = _format(value, level)
                allsimple = allsimple and simple
                if keywords:
                    args.append('%s=%s' % (name, value))
                else:
                    args.append(value)
            if include_attributes and node._attributes:
                for name in node._attributes:
                    try:
                        value = getattr(node, name)
                    except AttributeError:
                        continue
                    if value is None and getattr(cls, name, ...) is None:
                        continue
                    value, simple = _format(value, level)
                    allsimple = allsimple and simple
                    args.append('%s=%s' % (name, value))
            if allsimple and len(args) <= 3:
                return '%s(%s)' % (node.__class__.__name__, ', '.join(args)), not args
            return '%s(%s%s)' % (node.__class__.__name__, prefix, sep.join(args)), False
        elif isinstance(node, list):
            if not node:
                return '[]', True
            return '[%s%s]' % (prefix, sep.join(_format(x, level)[0] for x in node)), False
        return repr(node), True

    if not isinstance(node, AST):
        raise TypeError('expected AST, got %r' % node.__class__.__name__)
    if indent is not None and not isinstance(indent, str):
        indent = ' ' * indent
    return _format(node)[0]


def copy_location(new_node, old_node):
    """Copy source location from old_node to new_node and return new_node."""
    for attr in 'lineno', 'col_offset', 'end_lineno', 'end_col_offset':
        if attr in old_node._attributes and attr in new_node._attributes:
            value = getattr(old_node, attr, None)
            if value is not None or (
                hasattr(old_node, attr) and attr.startswith("end_")
            ):
                setattr(new_node, attr, value)
    return new_node


def fix_missing_locations(node):
    """Give every node without a location its parent's."""
    def _fix(node, lineno, col_offset, end_lineno, end_col_offset):
        if 'lineno' in node._attributes:
            if not hasattr(node, 'lineno'):
                node.lineno = lineno
            else:
                lineno = node.lineno
        if 'end_lineno' in node._attributes:
            if getattr(node, 'end_lineno', None) is None:
                node.end_lineno = end_lineno
            else:
                end_lineno = node.end_lineno
        if 'col_offset' in node._attributes:
            if not hasattr(node, 'col_offset'):
                node.col_offset = col_offset
            else:
                col_offset = node.col_offset
        if 'end_col_offset' in node._attributes:
            if getattr(node, 'end_col_offset', None) is None:
                node.end_col_offset = end_col_offset
            else:
                end_col_offset = node.end_col_offset
        for child in iter_child_nodes(node):
            _fix(child, lineno, col_offset, end_lineno, end_col_offset)
    _fix(node, 1, 0, 1, 0)
    return node


def increment_lineno(node, n=1):
    """Increment the line numbers of every node in the tree by n."""
    for child in walk(node):
        if isinstance(child, TypeIgnore):
            child.lineno = getattr(child, 'lineno', 0) + n
            continue
        if 'lineno' in child._attributes:
            child.lineno = getattr(child, 'lineno', 0) + n
        if (
            "end_lineno" in child._attributes
            and (end_lineno := getattr(child, "end_lineno", 0)) is not None
        ):
            child.end_lineno = end_lineno + n
    return node


def iter_fields(node):
    """Yield (fieldname, value) for each field of node that is present."""
    for field in node._fields:
        try:
            yield field, getattr(node, field)
        except AttributeError:
            pass


def iter_child_nodes(node):
    """Yield the direct child nodes of node."""
    for name, field in iter_fields(node):
        if isinstance(field, AST):
            yield field
        elif isinstance(field, list):
            for item in field:
                if isinstance(item, AST):
                    yield item


def get_docstring(node, clean=True):
    """The docstring of a function, class or module node, or None."""
    if not isinstance(node, (AsyncFunctionDef, FunctionDef, ClassDef, Module)):
        raise TypeError("%r can't have docstrings" % node.__class__.__name__)
    if not (node.body and isinstance(node.body[0], Expr)):
        return None
    node = node.body[0].value
    if isinstance(node, Constant) and isinstance(node.value, str):
        text = node.value
    else:
        return None
    if clean:
        text = _cleandoc(text)
    return text


def _cleandoc(doc):
    # inspect.cleandoc
    lines = doc.expandtabs().split('\n')
    margin = None
    for line in lines[1:]:
        content = len(line.lstrip(' '))
        if content:
            indent = len(line) - content
            margin = indent if margin is None else min(margin, indent)
    if margin is not None:
        lines = [lines[0].lstrip(' ')] + [line[margin:] for line in lines[1:]]
    else:
        lines[0] = lines[0].lstrip(' ')
    while lines and not lines[-1]:
        lines.pop()
    while lines and not lines[0]:
        lines.pop(0)
    return '\n'.join(lines)


def get_source_segment(source, node, *, padded=False):
    """Get the source code segment of the source that generated node."""
    try:
        if node.end_lineno is None or node.end_col_offset is None:
            return None
        lineno = node.lineno - 1
        end_lineno = node.end_lineno - 1
        col_offset = node.col_offset
        end_col_offset = node.end_col_offset
    except AttributeError:
        return None
    lines = []
    start = 0
    for i, ch in enumerate(source):
        if ch == '\n' or (ch == '\r' and source[i + 1:i + 2] != '\n'):
            lines.append(source[start:i + 1])
            start = i + 1
            if len(lines) > end_lineno:
                break
    else:
        lines.append(source[start:])
    if end_lineno == lineno:
        return lines[lineno].encode()[col_offset:end_col_offset].decode()
    if padded:
        padding = ''.join(c if c in '\f\t' else ' '
                          for c in lines[lineno].encode()[:col_offset].decode())
    else:
        padding = ''
    first = padding + lines[lineno].encode()[col_offset:].decode()
    last = lines[end_lineno].encode()[:end_col_offset].decode()
    lines = lines[lineno + 1:end_lineno]
    lines.insert(0, first)
    lines.append(last)
    return ''.join(lines)


def walk(node):
    """Yield every node in the tree, breadth first."""
    todo = [node]
    i = 0
    while i < len(todo):
        node = todo[i]
        i += 1
        todo.extend(iter_child_nodes(node))
        yield node


class NodeVisitor(object):
    """Walks the tree and calls a visitor function for every node found:
    `visit_` plus the class name, else `generic_visit`."""

    def visit(self, node):
        method = 'visit_' + node.__class__.__name__
        visitor = getattr(self, method, self.generic_visit)
        return visitor(node)

    def generic_visit(self, node):
        for field, value in iter_fields(node):
            if isinstance(value, list):
                for item in value:
                    if isinstance(item, AST):
                        self.visit(item)
            elif isinstance(value, AST):
                self.visit(value)


class NodeTransformer(NodeVisitor):
    """A NodeVisitor whose visitors return the node to keep, a replacement,
    a list of nodes, or None to remove it."""

    def generic_visit(self, node):
        for field, old_value in iter_fields(node):
            if isinstance(old_value, list):
                new_values = []
                for value in old_value:
                    if isinstance(value, AST):
                        value = self.visit(value)
                        if value is None:
                            continue
                        elif not isinstance(value, AST):
                            new_values.extend(value)
                            continue
                    new_values.append(value)
                old_value[:] = new_values
            elif isinstance(old_value, AST):
                new_node = self.visit(old_value)
                if new_node is None:
                    delattr(node, field)
                else:
                    setattr(node, field, new_node)
        return node
