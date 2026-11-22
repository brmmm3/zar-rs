#ifndef ZAR_TCP_STREAMING_NATIVE_H
#define ZAR_TCP_STREAMING_NATIVE_H

/* Shared POSIX TCP framing for the C and C++ examples. */
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#define ZAR_TCP_MAX 65536

static int zar_send_all(int fd, const void *data, size_t length) {
    const unsigned char *p = (const unsigned char *)data;
    while (length) {
        ssize_t n = send(fd, p, length, MSG_NOSIGNAL);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) return -1;
        p += n;
        length -= (size_t)n;
    }
    return 0;
}

static int zar_read_all(int fd, void *data, size_t length) {
    unsigned char *p = (unsigned char *)data;
    while (length) {
        ssize_t n = recv(fd, p, length, 0);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) return -1;
        p += n;
        length -= (size_t)n;
    }
    return 0;
}

static int zar_send_length(int fd, uint32_t length) {
    uint32_t value = htonl(length);
    return zar_send_all(fd, &value, sizeof(value));
}

static int zar_read_length(int fd, uint32_t *length) {
    uint32_t value;
    if (zar_read_all(fd, &value, sizeof(value))) return -1;
    *length = ntohl(value);
    return 0;
}

static int zar_frame(int fd, const unsigned char *data, size_t length) {
    while (length) {
        size_t size = length < ZAR_TCP_MAX ? length : ZAR_TCP_MAX;
        if (zar_send_length(fd, (uint32_t)size) || zar_send_all(fd, data, size))
            return -1;
        data += size;
        length -= size;
    }
    return 0;
}

static int zar_socket(const char *host, const char *port, int server) {
    struct addrinfo hints, *addresses = NULL, *address;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    int status = getaddrinfo(host, port, &hints, &addresses);
    if (status) {
        fprintf(stderr, "address: %s\n", gai_strerror(status));
        return -1;
    }
    int fd = -1;
    for (address = addresses; address; address = address->ai_next) {
        fd = socket(address->ai_family, address->ai_socktype, address->ai_protocol);
        if (fd < 0) continue;
        if (server) {
            int yes = 1;
            if (!setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes)) &&
                !bind(fd, address->ai_addr, address->ai_addrlen) && !listen(fd, 16))
                break;
        } else if (!connect(fd, address->ai_addr, address->ai_addrlen)) {
            break;
        }
        close(fd);
        fd = -1;
    }
    freeaddrinfo(addresses);
    if (fd < 0) perror(server ? "listen" : "connect");
    return fd;
}

typedef int (*zar_extractor)(const char *, const char *, int);

static int zar_serve(const char *host, const char *port, const char *archive,
                     zar_extractor extract) {
    int listener = zar_socket(host, port, 1);
    if (listener < 0) return 1;
    fprintf(stderr, "Listening on %s:%s\n", host, port);
    for (;;) {
        int conn = accept(listener, NULL, NULL);
        if (conn < 0) {
            if (errno == EINTR) continue;
            perror("accept");
            close(listener);
            return 1;
        }
        uint32_t size;
        char name[ZAR_TCP_MAX + 1];
        int ok = !zar_read_length(conn, &size) && size > 0 && size <= ZAR_TCP_MAX;
        if (ok) ok = !zar_read_all(conn, name, size) && !memchr(name, 0, size);
        if (ok) {
            name[size] = '\0';
            ok = !extract(archive, name, conn) && !zar_send_length(conn, 0);
        }
        if (!ok) fprintf(stderr, "Request failed; transfer not completed\n");
        close(conn);
    }
}

static int zar_receive(const char *host, const char *port, const char *entry,
                       const char *path) {
    size_t entry_length = strlen(entry);
    if (!entry_length || entry_length > ZAR_TCP_MAX) {
        fprintf(stderr, "Invalid entry path length\n");
        return 1;
    }
    int out = open(path, O_WRONLY | O_CREAT | O_EXCL, 0666);
    if (out < 0) { perror("open output"); return 1; }
    int conn = zar_socket(host, port, 0);
    int ok = conn >= 0 && !zar_send_length(conn, (uint32_t)entry_length) &&
             !zar_send_all(conn, entry, entry_length);
    unsigned char buffer[ZAR_TCP_MAX];
    while (ok) {
        uint32_t length;
        if (zar_read_length(conn, &length) || length > ZAR_TCP_MAX) { ok = 0; break; }
        if (!length) break;
        if (zar_read_all(conn, buffer, length)) { ok = 0; break; }
        size_t offset = 0;
        while (offset < length) {
            ssize_t n = write(out, buffer + offset, length - offset);
            if (n < 0 && errno == EINTR) continue;
            if (n <= 0) { ok = 0; break; }
            offset += (size_t)n;
        }
    }
    if (conn >= 0) close(conn);
    if (close(out)) ok = 0;
    if (!ok) {
        fprintf(stderr, "Transfer failed\n");
        if (unlink(path)) perror("remove partial output");
        return 1;
    }
    return 0;
}

static int zar_tcp_main(int argc, char **argv, zar_extractor extract) {
    if (argc == 5 && !strcmp(argv[1], "server"))
        return zar_serve(argv[2], argv[3], argv[4], extract);
    if (argc == 6 && !strcmp(argv[1], "client"))
        return zar_receive(argv[2], argv[3], argv[4], argv[5]);
    fprintf(stderr, "usage: %s server HOST PORT ARCHIVE | client HOST PORT ENTRY OUTPUT\n", argv[0]);
    return 1;
}
#endif
