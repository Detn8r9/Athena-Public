#include <stdlib.h>

// glibc 2.38 introduced these C23 entrypoints. Some prebuilt native libraries (notably C++)
// may reference them even when targeting older glibc. Ubuntu 22.04 (glibc 2.35) does not
// provide these symbols, so we shim them to the older equivalents.

long __isoc23_strtol(const char *nptr, char **endptr, int base) {
  return strtol(nptr, endptr, base);
}

long long __isoc23_strtoll(const char *nptr, char **endptr, int base) {
  return strtoll(nptr, endptr, base);
}

unsigned long __isoc23_strtoul(const char *nptr, char **endptr, int base) {
  return strtoul(nptr, endptr, base);
}

unsigned long long __isoc23_strtoull(const char *nptr, char **endptr, int base) {
  return strtoull(nptr, endptr, base);
}

