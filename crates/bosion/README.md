# Bosion

_Gather build information for verbose versions flags._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license] or [MIT](https://passcod.mit-license.org).
- Status: maintained.

[docs]: https://docs.rs/bosion
[license]: ../../LICENSE

## Quick start

In your `Cargo.toml`:

```toml
[build-dependencies]
bosion = "1.0.0"
```

In your `build.rs`:

```rust ,no_run
fn main() {
    bosion::gather();
}
```

In your `src/main.rs`:

```rust ,ignore
include!(env!("BOSION_PATH"));

fn main() {
    // default output, like rustc -Vv
    println!("{}", Bosion::long_version());

    // with additional fields
    println!("{}", Bosion::long_version_with(&[
        ("custom data", "value"),
        ("LLVM version", "15.0.6"),
    ]));

    // enabled features like +feature +an-other
    println!("{}", Bosion::CRATE_FEATURE_STRING);

    // the raw data
    println!("{}", Bosion::GIT_COMMIT_HASH);
    println!("{}", Bosion::GIT_COMMIT_SHORTHASH);
    println!("{}", Bosion::GIT_COMMIT_DATE);
    println!("{}", Bosion::GIT_COMMIT_DATETIME);
    println!("{}", Bosion::CRATE_VERSION);
    println!("{:?}", Bosion::CRATE_FEATURES);
    println!("{}", Bosion::BUILD_DATE);
    println!("{}", Bosion::BUILD_DATETIME);
}
```

## Advanced usage

Generating a struct with public visibility:

```rust ,no_run
// build.rs
bosion::gather_pub();
```

Customising the output file and struct names:

```rust ,no_run
// build.rs
bosion::gather_to("buildinfo.rs", "Build", /* public? */ false);
```

Outputting build-time environment variables instead of source:

```rust ,ignore
// build.rs
bosion::gather_to_env();

// src/main.rs
fn main() {
    println!("{}", env!("BOSION_GIT_COMMIT_HASH"));
    println!("{}", env!("BOSION_GIT_COMMIT_SHORTHASH"));
    println!("{}", env!("BOSION_GIT_COMMIT_DATE"));
    println!("{}", env!("BOSION_GIT_COMMIT_DATETIME"));
    println!("{}", env!("BOSION_BUILD_DATE"));
    println!("{}", env!("BOSION_BUILD_DATETIME"));
    println!("{}", env!("BOSION_CRATE_VERSION"));
    println!("{}", env!("BOSION_CRATE_FEATURES")); // comma-separated
}
```

Custom env prefix:

```rust ,no_run
// build.rs
bosion::gather_to_env_with_prefix("MYAPP_");
```

## Features

- `reproducible`: reads [`SOURCE_DATE_EPOCH`](https://reproducible-builds.org/docs/source-date-epoch/) (default).
- `git`: enables gathering git information (default).
- `std`: enables the `long_version_with` method (default).
  Specifically, this is about the downstream crate's std support, not Bosion's, which always requires std.

## Why not...?

- [bugreport](https://github.com/sharkdp/bugreport): runtime library, for bug information.
- [git-testament](https://github.com/kinnison/git-testament): uses the `git` CLI instead of gitoxide.
- [human-panic](https://github.com/rust-cli/human-panic): runtime library, for panics.
- [shadow-rs](https://github.com/baoyachi/shadow-rs): uses libgit2 instead of gitoxide, doesn't rebuild on git changes.
- [vergen](https://github.com/rustyhorde/vergen): uses the `git` CLI instead of gitoxide.

Bosion also requires no dependencies outside of build.rs.
