# watchexec-signals

_Watchexec's signal types._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license] or [MIT](https://passcod.mit-license.org).
- Status: maintained.

[docs]: https://docs.rs/watchexec-signals
[license]: ../../LICENSE

```rust ,no_run
use watchexec_signals::SubSignal;

fn main() {
    assert_eq!(SubSignal::from_str("SIGINT").unwrap(), SubSignal::Interrupt);
}
```
