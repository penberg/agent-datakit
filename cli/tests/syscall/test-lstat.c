#define _GNU_SOURCE
#include "test-common.h"
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>

int test_lstat(const char *base_path) {
    char path[512], link_path[512];
    struct stat st, st_link;
    int result;

    snprintf(path, sizeof(path), "%s/test.txt", base_path);
    snprintf(link_path, sizeof(link_path), "%s/test_link", base_path);

    /* Test 1: Create a symbolic link */
    unlink(link_path);  /* Remove if exists */
    result = symlink("test.txt", link_path);

    /* Skip symlink tests if symlink syscall is not supported */
    if (result < 0 && (errno == ENOSYS || errno == EOPNOTSUPP)) {
        printf("  (Skipping symlink tests - syscall not supported)\n");
    } else {
        TEST_ASSERT_ERRNO(result == 0, "symlink creation should succeed");

        /* Test 2: lstat on symlink should return link info */
        result = lstat(link_path, &st_link);
        TEST_ASSERT_ERRNO(result == 0, "lstat should succeed");
        TEST_ASSERT(S_ISLNK(st_link.st_mode), "lstat should identify symlink");

        /* Test 3: stat on symlink should follow the link */
        result = stat(link_path, &st);
        TEST_ASSERT_ERRNO(result == 0, "stat should succeed");
        TEST_ASSERT(S_ISREG(st.st_mode), "stat should follow symlink to regular file");

        /* Test 4: lstat and stat should differ on symlinks */
        TEST_ASSERT(!S_ISLNK(st.st_mode), "stat should not see symlink");
        TEST_ASSERT(st.st_size != st_link.st_size, "sizes should differ (link vs file)");
    }

    /* Test 5: lstat on regular file should behave like stat */
    struct stat st_regular, st_lstat;
    result = stat(path, &st_regular);
    TEST_ASSERT_ERRNO(result == 0, "stat on regular file should succeed");

    result = lstat(path, &st_lstat);
    TEST_ASSERT_ERRNO(result == 0, "lstat on regular file should succeed");

    TEST_ASSERT(st_regular.st_size == st_lstat.st_size, "stat and lstat should match on regular file");
    TEST_ASSERT(st_regular.st_ino == st_lstat.st_ino, "inodes should match");

    return 0;
}
