# watchexec-events

_Watchexec's event types._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license] or [MIT](https://passcod.mit-license.org).
- Status: maintained.

[docs]: https://docs.rs/watchexec-events
[license]: ../../LICENSE

This is particularly useful if you're building a tool that runs under Watchexec, and want to easily
read its events (with `--emit-events-to=json-file` and `--emit-events-to=json-stdin`).

```rust ,no_run
use watchexec_events::JsonFormat;

fn main() -> Result<()> {
    // open stdin
    // read json events from stdin
    // print them to stdout in debug
}
```

## Features

- `serde`: enables serde support.
- `notify`: use Notify's file event types (default).

If you disable `notify`, you'll get a leaner dependency tree that's still able to parse the entire
events, but isn't binary compatible with Notify. In most deserialisation usecases, this is fine, but
it's not the default to avoid surprises.