#define _GNU_SOURCE
#include "test-common.h"
#include <fcntl.h>
#include <unistd.h>

int test_dup2(const char *base_path) {
    char path[512];
    char buf[256];
    int fd, fd_target, fd_new;
    ssize_t n;

    snprintf(path, sizeof(path), "%s/test.txt", base_path);

    /* Test 1: Duplicate to specific fd */
    fd = open(path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open should succeed");

    fd_target = 100;  /* Use a high fd number */
    fd_new = dup2(fd, fd_target);
    TEST_ASSERT_ERRNO(fd_new == fd_target, "dup2 should return target fd");

    /* Test 2: Target fd should work */
    n = read(fd_target, buf, sizeof(buf) - 1);
    TEST_ASSERT_ERRNO(n > 0, "read from duplicated fd should succeed");
    buf[n] = '\0';

    close(fd_target);

    /* Test 3: dup2 to same fd should be no-op */
    fd_new = dup2(fd, fd);
    TEST_ASSERT_ERRNO(fd_new == fd, "dup2 to same fd should return same fd");

    /* Test 4: dup2 should close target fd if it was open */
    fd_target = open(path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd_target >= 0, "open should succeed");

    /* Reset fd position before dup2 */
    lseek(fd, 0, SEEK_SET);

    fd_new = dup2(fd, fd_target);
    TEST_ASSERT_ERRNO(fd_new == fd_target, "dup2 should succeed");

    /* Original fd_target is now closed and points to fd's file */
    n = read(fd_target, buf, sizeof(buf) - 1);
    TEST_ASSERT_ERRNO(n > 0, "read from dup2 target should succeed");

    close(fd);
    close(fd_target);

    /* Test 5: dup2 with closed source fd should fail */
    fd_new = dup2(fd, 101);
    TEST_ASSERT(fd_new < 0 && errno == EBADF, "dup2 with closed source should fail with EBADF");

    return 0;
}
