#!/usr/bin/env -S cargo +nightly -Zscript
```cargo
[dependencies]
bosion = { version = "*", path = "../.." }
```

fn main() {
	dbg!(bosion::Info::gather().unwrap());
}
