/* Stock public API only. No clocks. SOURCE is never rewritten.
 * Compile once, serialize trusted bytecode, then load/evaluate in two fresh
 * contexts within one live runtime. Loading is a separate, nonzero boundary;
 * it is not equivalent to simply reusing Monty's compiled Rust object.
 */
#include "quickjs.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static char output[16384];
static size_t used;

static int exception(JSContext *ctx, JSValue value) {
    if (!JS_IsException(value)) return 0;
    JSValue error = JS_GetException(ctx);
    const char *message = JS_ToCString(ctx, error);
    fprintf(stderr, "QuickJS exception: %s\n", message ? message : "<conversion failed>");
    JS_FreeCString(ctx, message);
    JS_FreeValue(ctx, error);
    return 1;
}

static JSValue log_line(JSContext *ctx, JSValueConst self, int argc, JSValueConst *argv) {
    (void)self;
    for (int i = 0; i < argc; i++) {
        size_t length;
        const char *part = JS_ToCStringLen(ctx, &length, argv[i]);
        if (!part) return JS_EXCEPTION;
        if (used + length + 2 >= sizeof(output)) {
            JS_FreeCString(ctx, part);
            return JS_ThrowRangeError(ctx, "probe output buffer exceeded");
        }
        if (i) output[used++] = ' ';
        memcpy(output + used, part, length);
        used += length;
        JS_FreeCString(ctx, part);
    }
    if (used + 1 >= sizeof(output)) return JS_ThrowRangeError(ctx, "probe output buffer exceeded");
    output[used++] = '\n';
    output[used] = 0;
    return JS_UNDEFINED;
}

static int install_console(JSContext *ctx) {
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue console = JS_NewObject(ctx);
    if (JS_SetPropertyStr(ctx, console, "log", JS_NewCFunction(ctx, log_line, "log", 1)) < 0) {
        JS_FreeValue(ctx, console);
        JS_FreeValue(ctx, global);
        return -1;
    }
    int result = JS_SetPropertyStr(ctx, global, "console", console);
    JS_FreeValue(ctx, global);
    return result;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: quickjs-warm SOURCE.js EXPECTED_STDOUT\n");
        return 2;
    }
    FILE *file = fopen(argv[1], "rb");
    if (!file || fseek(file, 0, SEEK_END)) return 2;
    long length = ftell(file);
    if (length < 0 || length > 1048576 || fseek(file, 0, SEEK_SET)) return 2;
    char *source = malloc((size_t)length + 1);
    if (!source || fread(source, 1, (size_t)length, file) != (size_t)length) return 2;
    fclose(file);
    source[length] = 0;
    fprintf(stderr, "boundary runtime: JS_NewRuntime\n");
    JSRuntime *runtime = JS_NewRuntime();
    if (!runtime) return 2;
    JSContext *compiler = JS_NewContext(runtime);
    if (!compiler) return 2;
    fprintf(stderr, "boundary compile: JS_Eval COMPILE_ONLY (no guest execution)\n");
    JSValue code = JS_Eval(compiler, source, (size_t)length, argv[1],
                           JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_COMPILE_ONLY);
    free(source);
    if (exception(compiler, code)) return 1;
    size_t bytes;
    fprintf(stderr, "boundary bytecode-cache: JS_WriteObject BYTECODE (not a live VM snapshot)\n");
    uint8_t *cache = JS_WriteObject(compiler, &bytes, code, JS_WRITE_OBJ_BYTECODE);
    JS_FreeValue(compiler, code);
    if (!cache) return 1;
    for (int index = 0; index < 2; index++) {
        fprintf(stderr, "boundary session %d: JS_NewContext + console.log host adapter\n", index);
        JSContext *ctx = JS_NewContext(runtime);
        if (!ctx || install_console(ctx) < 0) return 2;
        fprintf(stderr, "boundary load %d: JS_ReadObject BYTECODE\n", index);
        JSValue loaded = JS_ReadObject(ctx, cache, bytes, JS_READ_OBJ_BYTECODE);
        if (exception(ctx, loaded)) return 1;
        used = 0;
        output[0] = 0;
        fprintf(stderr, "boundary evaluate %d: JS_EvalFunction + host output collection\n", index);
        JSValue result = JS_EvalFunction(ctx, loaded);
        if (exception(ctx, result)) return 1;
        JS_FreeValue(ctx, result);
        if (used && output[used - 1] == '\n') output[--used] = 0;
        if (strcmp(output, argv[2])) {
            fprintf(stderr, "checksum mismatch: got [%s], expected [%s]\n", output, argv[2]);
            return 1;
        }
        printf("%d: %s\n", index, output);
        JS_FreeContext(ctx);
    }
    js_free(compiler, cache);
    JS_FreeContext(compiler);
    JS_FreeRuntime(runtime);
    return 0;
}
