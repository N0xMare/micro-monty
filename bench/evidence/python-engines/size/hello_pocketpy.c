// The minimal PocketPy host the size table measures: read one script
// (argv[1]), run it, print its exception if it raises.
#include <stdio.h>
#include <stdlib.h>
#include "pocketpy.h"

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    FILE *f = fopen(argv[1], "rb");
    if (!f) return 2;
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *src = malloc(n + 1);
    src[fread(src, 1, n, f)] = 0;
    fclose(f);
    py_initialize();
    bool ok = py_exec(src, argv[1], EXEC_MODE, NULL);
    if (!ok) py_printexc();
    py_finalize();
    return ok ? 0 : 1;
}
