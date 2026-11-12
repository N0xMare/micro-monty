# globals() (decision G2): reads of the module's globals from module code and
# functions: in, [], get, keys/items, iteration, a view that sees later names.
import json
x = 1
def submit_answer(a): return a
print("submit_answer" in globals(), "missing" in globals(), globals()["x"], globals().get("nope", 7), globals().get("x"))
def f():
    y = 2
    return "y" in globals(), "x" in globals(), "f" in globals()
print(f())
g = globals()
z = 3
print("z" in g, g["z"], g["json"] is json)
print(sorted(k for k in globals() if not k.startswith("__")))
print(len([k for k in globals().keys() if k == "x"]), [v for k, v in globals().items() if k == "x"])
if "submit_answer" in globals():
    print(submit_answer(5))
fn = globals().get("submit_answer")
print(fn(6) if callable(fn) else None)
try:
    globals()["missing"]
except KeyError as e:
    print("KeyError", e)
