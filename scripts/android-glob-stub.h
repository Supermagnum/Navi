/* Android NDK has no glob(3). Hamlib microham.c includes <glob.h>; replace with
 * this header from scripts/build-hamlib-android.sh so the shared library builds.
 * Discovery always reports no microHam devices (acceptable on Android).
 */
#ifndef NAVI_ANDROID_GLOB_STUB_H
#define NAVI_ANDROID_GLOB_STUB_H

#include <stddef.h>

typedef struct {
    size_t gl_pathc;
    char **gl_pathv;
    size_t gl_offs;
} glob_t;

#ifndef GLOB_NOMATCH
#define GLOB_NOMATCH 3
#endif

static inline int glob(
    const char *pattern,
    int flags,
    int (*errfunc)(const char *, int),
    glob_t *pglob
) {
    (void)pattern;
    (void)flags;
    (void)errfunc;
    if (pglob) {
        pglob->gl_pathc = 0;
        pglob->gl_pathv = NULL;
        pglob->gl_offs = 0;
    }
    return GLOB_NOMATCH;
}

static inline void globfree(glob_t *pglob) {
    (void)pglob;
}

#endif /* NAVI_ANDROID_GLOB_STUB_H */
