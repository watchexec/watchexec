# watchexec-events

_Watchexec's event types._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license] or [MIT](https://passcod.mit-license.org).
- Status: maintained.

[docs]: https://docs.rs/watchexec-events
[license]: ../../LICENSE

This is particularly useful if you're building a tool that runs under Watchexec, and want to easily
read its events.

## Quick start

```rust ,no_run
use watchexec_events::JsonFormat;

fn main() -> Result<()> {
    // open stdin
    // read json events from stdin
    // print them to stdout in debug
}
```
