#define _GNU_SOURCE
#include "test-common.h"
#include <fcntl.h>
#include <unistd.h>

int test_close(const char *base_path) {
    char path[512];
    char buf[256];
    int fd;
    ssize_t n;

    snprintf(path, sizeof(path), "%s/test.txt", base_path);

    /* Test 1: Close valid fd */
    fd = open(path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open should succeed");

    int result = close(fd);
    TEST_ASSERT_ERRNO(result == 0, "close should succeed");

    /* Test 2: Operations on closed fd should fail */
    n = read(fd, buf, sizeof(buf));
    TEST_ASSERT(n < 0 && errno == EBADF, "read after close should fail with EBADF");

    /* Test 3: Close already closed fd should fail */
    result = close(fd);
    TEST_ASSERT(result < 0 && errno == EBADF, "close on already closed fd should fail with EBADF");

    /* Test 4: Close invalid fd should fail */
    result = close(9999);
    TEST_ASSERT(result < 0 && errno == EBADF, "close on invalid fd should fail with EBADF");

    return 0;
}
