/* Public QuickJS/dquickts APIs only. Trusted bounded numeric workload.
 * Usage: numeric js|py N REPS [QPYHOME]. Build separately against each library.
 * The Python definition API fuses compilation and initialization. Calls reuse
 * the same function and context; conversion/checking belongs to call_ns.
 */
#define _POSIX_C_SOURCE 200809L
#include "quickjs.h"
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static uint64_t now(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t)) exit(2);
    return (uint64_t)t.tv_sec * 1000000000 + (uint64_t)t.tv_nsec;
}

static void fail(JSContext *ctx, const char *where) {
    JSValue error = JS_GetException(ctx);
    const char *text = JS_ToCString(ctx, error);
    fprintf(stderr, "%s: %s\n", where, text ? text : "<conversion failed>");
    JS_FreeCString(ctx, text);
    JS_FreeValue(ctx, error);
    exit(1);
}

static void call(JSContext *ctx, JSValueConst fn, JSValueConst arg, int64_t expected) {
    JSValue result = JS_Call(ctx, fn, JS_UNDEFINED, 1, &arg);
    if (JS_IsException(result)) fail(ctx, "call");
    int64_t got;
    if (JS_ToInt64Ext(ctx, &got, result) < 0) fail(ctx, "result conversion");
    JS_FreeValue(ctx, result);
    if (got != expected) {
        fprintf(stderr, "numeric result mismatch: %" PRId64 " != %" PRId64 "\n", got, expected);
        exit(1);
    }
}

int main(int argc, char **argv) {
    if (argc < 4 || argc > 5) return 2;
    int python = !strcmp(argv[1], "py");
    if (!python && strcmp(argv[1], "js")) return 2;
    char *end;
    uint64_t n = strtoull(argv[2], &end, 10);
    if (*end || n > 10000000) return 2;
    unsigned reps = (unsigned)strtoul(argv[3], &end, 10);
    if (*end || reps < 1 || reps > 1000) return 2;
    int64_t expected = (int64_t)(n * (n ? n - 1 : 0) / 2);
    uint64_t t = now();
    JSRuntime *rt = JS_NewRuntime();
    if (!rt) return 1;
    JSContext *ctx = JS_NewContext(rt);
    if (!ctx) return 1;
#ifdef EVIDENCE_PYTHON
    if (python) {
        if (argc != 5) return 2;
        JSPythonConfig cfg = {0};
        cfg.home = argv[4];
        cfg.hash_seed = "0";
        cfg.isolated = 1;
        if (JS_InitPython(ctx, &cfg) < 0) fail(ctx, "Python initialization");
    }
#else
    if (python) return 2;
#endif
    uint64_t setup_ns = now() - t;
    const char *js = "function numericLoop(n) { let total = 0; for (let i = 0; i < n; i++) total += i; return total; } numericLoop;";
    JSValue fn;
    t = now();
#ifdef EVIDENCE_PYTHON
    if (python) {
        const char *py = "def numeric_loop(n):\n    total = 0\n    for i in range(n):\n        total += i\n    return total\n";
        JSValue v = JS_EvalPython(ctx, py, strlen(py), "numeric.py", JS_PY_EVAL_EXEC);
        if (JS_IsException(v)) fail(ctx, "Python definition");
        JS_FreeValue(ctx, v);
        fn = JS_EvalPython(ctx, "numeric_loop", 12, "numeric.py", JS_PY_EVAL_EVAL);
    } else
#endif
    {
        fn = JS_Eval(ctx, js, strlen(js), "numeric.js", JS_EVAL_TYPE_GLOBAL);
    }
    if (JS_IsException(fn)) fail(ctx, "function lookup");
    uint64_t compile_initialize_ns = now() - t;
    JSValue arg = JS_NewInt64(ctx, (int64_t)n);
    t = now();
    call(ctx, fn, arg, expected);
    uint64_t first_ns = now() - t;
    uint64_t times[1000];
    for (unsigned i = 0; i < reps; i++) {
        t = now();
        call(ctx, fn, arg, expected);
        times[i] = now() - t;
    }
    printf("{\"n\":%" PRIu64 ",\"checked_result\":%" PRId64 ",\"setup_ns\":%" PRIu64 ",\"compile_initialize_ns\":%" PRIu64 ",\"first_ns\":%" PRIu64 ",\"warm_ns\":[", n, expected, setup_ns, compile_initialize_ns, first_ns);
    for (unsigned i = 0; i < reps; i++) printf("%s%" PRIu64, i ? "," : "", times[i]);
    puts("]}");
    JS_FreeValue(ctx, arg);
    JS_FreeValue(ctx, fn);
    JS_FreeContext(ctx);
    JS_FreeRuntime(rt);
    return 0;
}
