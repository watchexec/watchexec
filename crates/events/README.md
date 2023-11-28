# watchexec-events

_Watchexec's event types._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license] or [MIT](https://passcod.mit-license.org).
- Status: maintained.

[docs]: https://docs.rs/watchexec-events
[license]: ../../LICENSE

Fundamentally, events in watchexec have three purposes:

1. To trigger the launch, restart, or other interruption of a process;
2. To be filtered upon according to whatever set of criteria is desired;
3. To carry information about what caused the event, which may be provided to the process.

Outside of Watchexec, this library is particularly useful if you're building a tool that runs under
it, and want to easily read its events (with `--emit-events-to=json-file` and `--emit-events-to=json-stdio`).

```rust ,no_run
use std::io::{stdin, Result};
use watchexec_events::Event;

fn main() -> Result<()> {
    for line in stdin().lines() {
        let event: Event = serde_json::from_str(&line?)?;
        dbg!(event);
    }

    Ok(())
}
```

## Features

- `serde`: enables serde support.
- `notify`: use Notify's file event types (default).

If you disable `notify`, you'll get a leaner dependency tree that's still able to parse the entire
events, but isn't type compatible with Notify. In most deserialisation usecases, this is fine, but
it's not the default to avoid surprises.
