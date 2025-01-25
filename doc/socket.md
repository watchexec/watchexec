# `--socket`, `systemd.socket`, `systemfd`

The `--socket` option is a lightweight version of [the `systemfd` tool][systemfd], which itself is
an implementation of [systemd's socket activation feature][systemd sockets], which itself is a
reimagination of earlier socket activation efforts, such as inetd and launchd.

All three of these are compatible with each other in some ways.
This document attempts to describe the commonalities and specify minimum behaviour that additional implementations should follow to keep compatibility.
It does not seek to establish authority over any project.

[systemfd]: https://github.com/mitsuhiko/systemfd
[systemd sockets]: https://0pointer.de/blog/projects/socket-activation.html

## Basic principle of operation

There's two programs involved: a socket provider, and a socket consumer.

In systemd, the provider is systemd itself, and the consumer is the main service process.
In watchexec (and systemfd), the provider is watchexec itself, and the consumer is the command it runs.

The provider creates a socket and binds them to an address, and then makes it available to the consumer.
There is an optional authentication layer to avoid the wrong process from attaching to the wrong socket.
The consumer that obtains a socket is then able to listen on it.
When the consumer exits, it doesn't close the socket; the provider then makes it available to the next instance.

Socket activation is an advanced behaviour, where the provider listens on the socket itself and uses that to start the consumer service.
As the provider controls the socket, more behaviours are possible such as having the real address bound to a separate socket and passing data through, or providing new sockets instead of sharing a single one.
The important principle is that the consumer should not need to care: socket control is decoupled from application message and stream handling.

## Unix

The Unix protocol was designed by systemd.

Sockets are provided to consumers through file descriptors.

- The file descriptors are assigned in a contiguous block.
- The number of socket file descriptors is passed to the consumer using the environment variable `LISTEN_FDS`.
- The starting file descriptor is read from the environment variable `LISTEN_FDS_FIRST_FD`, or defaults to `3` if that variable is not present.
- If the `LISTEN_PID` environment variable is present, and the process ID of the consumer process doesn't match it, it must stop and not listen on any of the file descriptors.
- The consumer may choose to reject the sockets if the file descriptor count isn't what it expects.
- The consumer should strip the above environment variables from any child process it starts.

The consumer side in pseudo code:

```
let pid_check = env::get("LISTEN_PID");
if pid_check && pid_check != getpid() {
    return;
}

let expected_socket_count = 2;
let fd_count = env::get("LISTEN_FDS");
if !fd_count || fd_count != expected_socket_count {
    return;
}

let starting_fd = env::get("LISTEN_FDS_FIRST_FD");
if !starting_fd {
    starting_fd = 3;
}

for (let fd = starting_fd; fd += 1; fd < starting_fd + fd_count) {
    configure_socket(fd);
}
```

## Windows

The Windows protocol was designed by systemfd.

Sockets are provided to consumers through the [WSAPROTOCOL_INFOW] structure.

- The provider starts a TCP server bound to 127.0.0.1 on a random port.
  - It writes the address to the server to the `SYSTEMFD_SOCKET_SERVER` environment variable for the consumer processes.
- The provider generates and stores a random 128 bit value as a key for a socket set.
  - It writes the key in UUID hex string format (e.g. `59fb60fe-2634-4ec8-aa81-038793888c8e`) to the `SYSTEMFD_SOCKET_SECRET` environment variable for the consumer processes.
- The consumer opens a connection to the `SYSTEMFD_SOCKET_SERVER` and:
  1. reads the key from `SYSTEMFD_SOCKET_SECRET`;
  2. writes the key in the same format, then a `|` character, then its own process ID as a string (in base 10), and then EOF;
  2. reads the response to EOF.
- The response will be one or more `WSAPROTOCOL_INFOW` structures, with no padding or separators.
- If the provider has no record of the key (i.e. if it doesn't match the one provided to the consumer via `SYSTEMFD_SOCKET_SECRET`), it will close the connection without sending any data.
- Optionally, the provider can check the consumer's PID is what it expects, and reject if it's unhappy (by closing the connection without sending any data).

The consumer side in pseudo code:

```
let server = env::get("SYSTEMFD_SOCKET_SERVER");
let key = env::get("SYSTEMFD_SOCKET_SECRET");

if !server || !key {
    return;
}

if !valid_uuid(key) {
    return;
}

let (writer, reader) = TcpClient::connect(server);
writer.write(key);
writer.write("|");
writer.write(getpid().to_string());
writer.close();

while reader.has_more_data() {
    let socket = reader.read(size_of(WSAPROTOCOL_INFOW)) as WSAPROTOCOL_INFOW;
    configure_socket(socket);
}
```

[WSAPROTOCOL_INFOW]: https://learn.microsoft.com/en-us/windows/win32/api/winsock2/ns-winsock2-wsaprotocol_infow
