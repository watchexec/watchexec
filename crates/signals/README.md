# watchexec-signals

_Watchexec's signal type._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license] or [MIT](https://passcod.mit-license.org).
- Status: maintained.

[docs]: https://docs.rs/watchexec-signals
[license]: ../../LICENSE

```rust
use watchexec_signals::Signal;

fn main() {
    assert_eq!(Signal::from_str("SIGINT").unwrap(), Signal::Interrupt);
}
```

## Features

- `serde`: enables serde support.
- `fromstr`: enables `FromStr` support (default).
- `miette`: enables miette (rich diagnostics) support (default).
