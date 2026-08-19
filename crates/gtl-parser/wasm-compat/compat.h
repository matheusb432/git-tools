#ifndef GTL_TREE_SITTER_WASM_COMPAT_H
#define GTL_TREE_SITTER_WASM_COMPAT_H

#include <stdint.h>

static inline int gtl_tree_sitter_isdigit(int character) {
    return character >= '0' && character <= '9';
}

static inline int gtl_tree_sitter_strcmp(const char *left, const char *right) {
    while (*left != '\0' && *left == *right) {
        left++;
        right++;
    }
    return *(const unsigned char *)left - *(const unsigned char *)right;
}

static inline char *gtl_tree_sitter_strncpy(
    char *restrict destination,
    const char *restrict source,
    size_t count
) {
    size_t index = 0;
    while (index < count && source[index] != '\0') {
        destination[index] = source[index];
        index++;
    }
    while (index < count) {
        destination[index++] = '\0';
    }
    return destination;
}

static inline int gtl_tree_sitter_towlower(int character) {
    return character >= 'A' && character <= 'Z' ? character + ('a' - 'A') : character;
}

static inline int gtl_tree_sitter_towupper(int character) {
    return character >= 'a' && character <= 'z' ? character - ('a' - 'A') : character;
}

void *gtl_tree_sitter_malloc(size_t size);
void *gtl_tree_sitter_calloc(size_t count, size_t size);
void *gtl_tree_sitter_realloc(void *pointer, size_t size);
void gtl_tree_sitter_free(void *pointer);

#define isdigit gtl_tree_sitter_isdigit
#define strcmp gtl_tree_sitter_strcmp
#define strncpy gtl_tree_sitter_strncpy
#define towlower gtl_tree_sitter_towlower
#define towupper gtl_tree_sitter_towupper
#define malloc gtl_tree_sitter_malloc
#define calloc gtl_tree_sitter_calloc
#define realloc gtl_tree_sitter_realloc
#define free gtl_tree_sitter_free

#endif
