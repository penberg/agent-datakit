#define _GNU_SOURCE
#include "test-common.h"
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>

int test_fstat(const char *base_path) {
    char path[512];
    struct stat st;
    int fd, result;

    snprintf(path, sizeof(path), "%s/test.txt", base_path);

    /* Test 1: fstat on open file */
    fd = open(path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open should succeed");

    result = fstat(fd, &st);
    TEST_ASSERT_ERRNO(result == 0, "fstat should succeed");
    TEST_ASSERT(S_ISREG(st.st_mode), "should be a regular file");
    TEST_ASSERT(st.st_size > 0, "file size should be greater than 0");

    /* Test 2: Compare fstat with stat */
    struct stat st2;
    result = stat(path, &st2);
    TEST_ASSERT_ERRNO(result == 0, "stat should succeed");
    TEST_ASSERT(st.st_size == st2.st_size, "fstat and stat should return same size");
    TEST_ASSERT(st.st_ino == st2.st_ino, "fstat and stat should return same inode");

    close(fd);

    /* Test 3: fstat on closed fd should fail */
    result = fstat(fd, &st);
    TEST_ASSERT(result < 0 && errno == EBADF, "fstat on closed fd should fail with EBADF");

    /* Test 4: fstat on directory fd */
    fd = open(base_path, O_RDONLY | O_DIRECTORY);
    TEST_ASSERT_ERRNO(fd >= 0, "open directory should succeed");

    result = fstat(fd, &st);
    TEST_ASSERT_ERRNO(result == 0, "fstat on directory fd should succeed");
    TEST_ASSERT(S_ISDIR(st.st_mode), "should be a directory");

    close(fd);

    return 0;
}
