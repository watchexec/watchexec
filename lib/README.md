[![Crates.io page](https://badgen.net/crates/v/watchexec)](https://crates.io/crates/watchexec)
[![API Docs](https://docs.rs/watchexec/badge.svg)][docs]
[![Crate license: Apache 2.0](https://badgen.net/badge/license/Apache%202.0)][license]
![MSRV: 1.43.0 (breaking)](https://badgen.net/badge/MSRV/1.43.0%20%28breaking%29/green)
[![CI status](https://github.com/watchexec/watchexec/actions/workflows/check.yml/badge.svg)](https://github.com/watchexec/watchexec/actions/workflows/check.yml)

# Watchexec library

_The library which powers [Watchexec CLI](https://github.com/watchexec/watchexec) and other tools._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license].
- Minimum Supported Rust Version: 1.43.0.

[docs]: https://docs.rs/watchexec
[license]: ../LICENSE


## Quick start

```rust
use watchexec::{
    config::ConfigBuilder,
    error::Result,
    pathop::PathOp,
    run::{
        ExecHandler,
        Handler,
        watch,
    },
};

fn main() -> Result<()> {
    let config = ConfigBuilder::default()
        .clear_screen(true)
        .run_initially(true)
        .paths(vec![ "/path/to/dir".into() ])
        .cmd(vec![ "date; seq 1 10".into() ])
        .build()?;

    let handler = MyHandler(ExecHandler::new(options)?);
    watch(&handler)
}

struct MyHandler(ExecHandler);

impl Handler for MyHandler {
    fn args(&self) -> Config {
        self.0.args()
    }

    fn on_manual(&self) -> Result<bool> {
        println!("Running manually!");
        self.0.on_manual()
    }

    fn on_update(&self, ops: &[PathOp]) -> Result<bool> {
        println!("Running manually {:?}", ops);
        self.0.on_update(ops)
    }
}
```
