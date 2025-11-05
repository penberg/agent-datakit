#define _GNU_SOURCE
#include "test-common.h"
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>

int test_stat(const char *base_path) {
    char path[512];
    struct stat st;
    int result;

    /* Test 1: stat on existing file */
    snprintf(path, sizeof(path), "%s/test.txt", base_path);
    result = stat(path, &st);
    TEST_ASSERT_ERRNO(result == 0, "stat should succeed on existing file");
    TEST_ASSERT(S_ISREG(st.st_mode), "test.txt should be a regular file");
    TEST_ASSERT(st.st_size > 0, "file size should be greater than 0");

    /* Test 2: stat on directory */
    result = stat(base_path, &st);
    TEST_ASSERT_ERRNO(result == 0, "stat should succeed on directory");
    TEST_ASSERT(S_ISDIR(st.st_mode), "base_path should be a directory");

    /* Test 3: stat on non-existent file should fail */
    snprintf(path, sizeof(path), "%s/nonexistent.txt", base_path);
    result = stat(path, &st);
    TEST_ASSERT(result < 0 && errno == ENOENT, "stat should fail with ENOENT on non-existent file");

    /* Test 4: Create a file and verify its stat */
    snprintf(path, sizeof(path), "%s/newfile.txt", base_path);
    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    TEST_ASSERT_ERRNO(fd >= 0, "create file should succeed");
    write(fd, "test", 4);
    close(fd);

    result = stat(path, &st);
    TEST_ASSERT_ERRNO(result == 0, "stat should succeed on created file");
    TEST_ASSERT(st.st_size == 4, "file size should be 4 bytes");
    TEST_ASSERT((st.st_mode & 0777) == 0644, "file permissions should be 0644");

    return 0;
}
