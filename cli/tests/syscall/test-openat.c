#define _GNU_SOURCE
#include "test-common.h"
#include <fcntl.h>
#include <unistd.h>

int test_openat(const char *base_path) {
    char path[512];
    int fd, dirfd;

    /* Test 1: Open with AT_FDCWD (current directory) */
    snprintf(path, sizeof(path), "%s/test.txt", base_path);
    fd = openat(AT_FDCWD, path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "openat with AT_FDCWD should succeed");
    close(fd);

    /* Test 2: Open relative to directory fd */
    dirfd = open(base_path, O_RDONLY | O_DIRECTORY);
    TEST_ASSERT_ERRNO(dirfd >= 0, "open directory should succeed");

    fd = openat(dirfd, "test.txt", O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "openat with directory fd should succeed");
    close(fd);
    close(dirfd);

    /* Test 3: Open with O_CREAT flag */
    snprintf(path, sizeof(path), "%s/created.txt", base_path);
    fd = openat(AT_FDCWD, path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    TEST_ASSERT_ERRNO(fd >= 0, "openat with O_CREAT should succeed");
    close(fd);

    /* Test 4: Open non-existent file without O_CREAT should fail */
    snprintf(path, sizeof(path), "%s/nonexistent.txt", base_path);
    fd = openat(AT_FDCWD, path, O_RDONLY);
    TEST_ASSERT(fd < 0 && errno == ENOENT, "openat non-existent file should fail with ENOENT");

    return 0;
}
