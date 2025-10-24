#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <string.h>

int main() {
    char buf[256];
    int fd, fd2, n;

    // Test 1: Open and read
    fd = open("/sandbox/test.txt", O_RDONLY);
    if (fd < 0) {
        fprintf(stderr, "FAIL: open failed\n");
        return 1;
    }
    printf("Opened file, got FD: %d\n", fd);

    n = read(fd, buf, sizeof(buf) - 1);
    if (n < 0) {
        fprintf(stderr, "FAIL: read failed\n");
        return 1;
    }
    buf[n] = '\0';
    printf("Read: %s", buf);

    // Test 2: Dup
    fd2 = dup(fd);
    if (fd2 < 0) {
        fprintf(stderr, "FAIL: dup failed\n");
        return 1;
    }
    printf("Dup'd FD: %d -> %d\n", fd, fd2);

    // Test 3: Close
    if (close(fd) < 0) {
        fprintf(stderr, "FAIL: close fd failed\n");
        return 1;
    }
    printf("Closed FD: %d\n", fd);

    if (close(fd2) < 0) {
        fprintf(stderr, "FAIL: close fd2 failed\n");
        return 1;
    }
    printf("Closed FD: %d\n", fd2);

    // Test 4: Write
    fd = open("/sandbox/output.txt", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) {
        fprintf(stderr, "FAIL: open for write failed\n");
        return 1;
    }

    const char *msg = "Written via virtual FD\n";
    n = write(fd, msg, strlen(msg));
    if (n != strlen(msg)) {
        fprintf(stderr, "FAIL: write failed\n");
        return 1;
    }
    printf("Wrote %d bytes\n", n);

    close(fd);

    printf("All tests passed!\n");
    return 0;
}
