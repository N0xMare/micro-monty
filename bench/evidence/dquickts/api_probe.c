/* Diagnostic only: public quickjs.h APIs, no engine internals or timing.
 * Build against feat/py's pure-Python libquickjs.a. Each invocation creates
 * one fresh runtime and tests one entry path; see README.md for boundaries.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "quickjs.h"

static unsigned polls;
static unsigned gate_calls;

static int request_once(JSRuntime *rt, void *opaque)
{
    (void)opaque;
    if (++polls == 1)
        JS_RequestSuspend(rt);
    return 0;
}

static JSValue host_gate(JSContext *ctx, JSValueConst self,
                         int argc, JSValueConst *argv)
{
    (void)self;
    (void)argc;
    (void)argv;
    ++gate_calls;
    return JS_SuspendCall(ctx);
}

static JSValue py(JSContext *ctx, const char *src, int flags)
{
    return JS_EvalPython(ctx, src, strlen(src), "<evidence-probe>", flags);
}

static JSValue js(JSContext *ctx, const char *src)
{
    return JS_Eval(ctx, src, strlen(src), "<evidence-probe>", JS_EVAL_TYPE_GLOBAL);
}

static void show_value(JSContext *ctx, const char *label, JSValueConst value)
{
    const char *s = JS_ToCString(ctx, value);
    printf("%s=%s\n", label, s ? s : "<conversion failed>");
    JS_FreeCString(ctx, s);
}

static void show_exception(JSContext *ctx)
{
    JSValue error = JS_GetException(ctx);
    show_value(ctx, "exception", error);
    JS_FreeValue(ctx, error);
}

int main(int argc, char **argv)
{
    const char *mode;
    int python, call, wrapper, gate, is_exception, suspended;
    JSRuntime *rt;
    JSContext *ctx;
    JSValue value, entry = JS_UNDEFINED, global;
    JSPythonConfig cfg = {0};
    const char *loop_setup =
        "def work():\n"
        "    total = 0\n"
        "    i = 0\n"
        "    while i < 100000:\n"
        "        total += i\n"
        "        i += 1\n"
        "    return total\n";
    const char *gate_setup =
        "from js import host_gate\n"
        "def work():\n"
        "    return host_gate() + 1\n";

    if (argc != 3) {
        fprintf(stderr, "usage: %s PYTHON_HOME {js-eval,py-eval,py-call,js-to-py}-{loop,gate}\n", argv[0]);
        return 2;
    }
    mode = argv[2];
    gate = strstr(mode, "-gate") != NULL;
    wrapper = strncmp(mode, "js-to-py-", 9) == 0;
    call = strncmp(mode, "py-call-", 8) == 0 || wrapper;
    python = strncmp(mode, "py-", 3) == 0 || wrapper;
    if ((!gate && !strstr(mode, "-loop")) ||
        (!python && strncmp(mode, "js-eval-", 8) != 0))
        return 2;

    rt = JS_NewRuntime();
    if (!rt)
        return 1;
    ctx = JS_NewContext(rt);
    if (!ctx) {
        JS_FreeRuntime(rt);
        return 1;
    }
    printf("mode=%s\n", mode);
    global = JS_GetGlobalObject(ctx);
    if (JS_SetPropertyStr(ctx, global, "host_gate",
                           JS_NewCFunction(ctx, host_gate, "host_gate", 0)) < 0)
        goto setup_error;
    if (python) {
        cfg.home = argv[1];
        cfg.hash_seed = "0";
        cfg.isolated = 1;
        if (JS_InitPython(ctx, &cfg) < 0)
            goto setup_error;
    }
    if (call) {
        value = py(ctx, gate ? gate_setup : loop_setup, JS_PY_EVAL_EXEC);
        if (JS_IsException(value))
            goto setup_error;
        JS_FreeValue(ctx, value);
        entry = py(ctx, "work", JS_PY_EVAL_EVAL);
        if (JS_IsException(entry))
            goto setup_error;
        if (wrapper && JS_SetPropertyStr(ctx, global, "py_entry", JS_DupValue(ctx, entry)) < 0)
            goto setup_error;
    }
    JS_FreeValue(ctx, global);
    global = JS_UNDEFINED;

    /* Install only after initialization, imports and function lookup. */
    if (!gate)
        JS_SetInterruptHandler(rt, request_once, NULL);
    if (wrapper) {
        value = js(ctx, "py_entry()");
    } else if (call) {
        value = JS_Call(ctx, entry, JS_UNDEFINED, 0, NULL);
    } else if (python) {
        value = py(ctx, gate ?
            "from js import host_gate\nanswer = host_gate() + 1\n" :
            "total = 0\ni = 0\nwhile i < 100000:\n    total += i\n    i += 1\n",
            JS_PY_EVAL_EXEC);
    } else {
        value = js(ctx, gate ? "host_gate() + 1" :
            "var total = 0; for (var i = 0; i < 100000; ++i) total += i; total");
    }
    is_exception = JS_IsException(value);
    suspended = JS_IsSuspended(rt);
    printf("initial exception=%d suspended=%d marker=%d polls=%u gate_calls=%u\n",
           is_exception, suspended, JS_IsSuspendException(ctx), polls, gate_calls);
    JS_SetInterruptHandler(rt, NULL, NULL);
    if (suspended) {
        JS_FreeValue(ctx, value);
        value = gate ? JS_ResumeWith(ctx, JS_NewInt32(ctx, 41), 0) : JS_Resume(ctx);
        is_exception = JS_IsException(value);
        printf("resume exception=%d suspended=%d polls=%u gate_calls=%u\n",
               is_exception, JS_IsSuspended(rt), polls, gate_calls);
    }
    if (is_exception) {
        show_exception(ctx);
    } else {
        show_value(ctx, "completion", value);
        if (python && !call) {
            JS_FreeValue(ctx, value);
            value = py(ctx, gate ? "answer" : "total", JS_PY_EVAL_EVAL);
            if (JS_IsException(value))
                show_exception(ctx);
            else
                show_value(ctx, "python_result", value);
        }
    }
    JS_FreeValue(ctx, value);
    JS_FreeValue(ctx, entry);
    JS_FreeContext(ctx);
    JS_FreeRuntime(rt);
    /* A guest TypeError is an observation, not a probe infrastructure failure. */
    return 0;

setup_error:
    show_exception(ctx);
    JS_FreeValue(ctx, global);
    JS_FreeValue(ctx, entry);
    JS_FreeContext(ctx);
    JS_FreeRuntime(rt);
    return 1;
}
