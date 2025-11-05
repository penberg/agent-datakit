#define _GNU_SOURCE
#include "test-common.h"
#include <fcntl.h>
#include <unistd.h>

int test_dup(const char *base_path) {
    char path[512];
    char buf1[256], buf2[256];
    int fd, fd_dup;
    ssize_t n1, n2;

    snprintf(path, sizeof(path), "%s/test.txt", base_path);

    /* Test 1: Duplicate file descriptor */
    fd = open(path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open should succeed");

    fd_dup = dup(fd);
    TEST_ASSERT_ERRNO(fd_dup >= 0, "dup should succeed");
    TEST_ASSERT(fd_dup != fd, "duplicated fd should be different from original");

    /* Test 2: Both fds should read the same file */
    n1 = read(fd, buf1, sizeof(buf1) - 1);
    TEST_ASSERT_ERRNO(n1 > 0, "read from original fd should succeed");
    buf1[n1] = '\0';

    /* Reset position for duplicated fd */
    lseek(fd_dup, 0, SEEK_SET);
    n2 = read(fd_dup, buf2, sizeof(buf2) - 1);
    TEST_ASSERT_ERRNO(n2 > 0, "read from duplicated fd should succeed");
    buf2[n2] = '\0';

    TEST_ASSERT(strcmp(buf1, buf2) == 0, "both fds should read same content");

    /* Test 3: Closing original fd shouldn't affect duplicated fd */
    close(fd);
    n1 = read(fd, buf1, sizeof(buf1));
    TEST_ASSERT(n1 < 0 && errno == EBADF, "read from closed original fd should fail");

    lseek(fd_dup, 0, SEEK_SET);
    n2 = read(fd_dup, buf2, sizeof(buf2) - 1);
    TEST_ASSERT_ERRNO(n2 > 0, "read from duplicated fd should still work");

    close(fd_dup);

    /* Test 4: dup on closed fd should fail */
    fd_dup = dup(fd);
    TEST_ASSERT(fd_dup < 0 && errno == EBADF, "dup on closed fd should fail with EBADF");

    return 0;
}
