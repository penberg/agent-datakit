#define _GNU_SOURCE
#include "test-common.h"
#include <fcntl.h>
#include <unistd.h>

int test_read(const char *base_path) {
    char path[512];
    char buf[256];
    int fd;
    ssize_t n;

    snprintf(path, sizeof(path), "%s/test.txt", base_path);

    /* Test 1: Read from file */
    fd = open(path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open should succeed");

    n = read(fd, buf, sizeof(buf) - 1);
    TEST_ASSERT_ERRNO(n > 0, "read should return positive bytes");
    buf[n] = '\0';

    TEST_ASSERT(strlen(buf) > 0, "read should return non-empty data");
    close(fd);

    /* Test 2: Read with offset using lseek */
    fd = open(path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open should succeed");

    off_t offset = lseek(fd, 5, SEEK_SET);
    TEST_ASSERT_ERRNO(offset == 5, "lseek should set offset to 5");

    n = read(fd, buf, 10);
    TEST_ASSERT_ERRNO(n > 0, "read after lseek should succeed");
    close(fd);

    /* Test 3: Read from closed fd should fail */
    n = read(fd, buf, sizeof(buf));
    TEST_ASSERT(n < 0 && errno == EBADF, "read from closed fd should fail with EBADF");

    return 0;
}
