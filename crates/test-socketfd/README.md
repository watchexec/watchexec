This is a testing tool for the `--socket` option, which can also be used by third-parties to check compatibility.

## Install

```console
cargo install --git https://github.com/watchexec/watchexec test-socketfd
```

## Usage

Print the control env variables and the number of available sockets:

```
test-socketfd
```

Validate that one TCP socket and one UDP socket are available, in this order:

```
test-socketfd tcp udp
```

The tool also supports `unix-stream`, `unix-datagram`, and `unix-raw` on unix, even if watchexec itself doesn't.
These correspond to the `ListenFd` methods here: https://docs.rs/listenfd/latest/listenfd/struct.ListenFd.html
