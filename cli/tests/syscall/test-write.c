#define _GNU_SOURCE
#include "test-common.h"
#include <fcntl.h>
#include <unistd.h>

int test_write(const char *base_path) {
    char path[512];
    char write_buf[] = "Hello from write test!";
    char read_buf[256];
    int fd;
    ssize_t n;

    snprintf(path, sizeof(path), "%s/output.txt", base_path);

    /* Test 1: Write to new file */
    fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    TEST_ASSERT_ERRNO(fd >= 0, "open for write should succeed");

    n = write(fd, write_buf, strlen(write_buf));
    TEST_ASSERT_ERRNO(n == (ssize_t)strlen(write_buf), "write should write all bytes");
    close(fd);

    /* Test 2: Read back and verify */
    fd = open(path, O_RDONLY);
    TEST_ASSERT_ERRNO(fd >= 0, "open for read should succeed");

    n = read(fd, read_buf, sizeof(read_buf) - 1);
    TEST_ASSERT_ERRNO(n > 0, "read should succeed");
    read_buf[n] = '\0';

    TEST_ASSERT(strcmp(read_buf, write_buf) == 0, "read data should match written data");
    close(fd);

    /* Test 3: Append to file */
    fd = open(path, O_WRONLY | O_APPEND);
    TEST_ASSERT_ERRNO(fd >= 0, "open for append should succeed");

    n = write(fd, " Appended!", 10);
    TEST_ASSERT_ERRNO(n == 10, "append write should succeed");
    close(fd);

    /* Test 4: Write to closed fd should fail */
    n = write(fd, write_buf, strlen(write_buf));
    TEST_ASSERT(n < 0 && errno == EBADF, "write to closed fd should fail with EBADF");

    return 0;
}
