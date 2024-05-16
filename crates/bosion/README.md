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
bosion = "1.1.0"
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
    println!("{}", Bosion::LONG_VERSION);

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

Bosion also requires no dependencies outside of build.rs, and was specifically made for crates
installed in a variety of ways, like with `cargo install`, from pre-built binary, from source with
git, or from source without git (like a tarball), on a variety of platforms. Its default output with
[clap](https://clap.rs) is almost exactly like `rustc -Vv`.

## Examples

The [examples](./examples) directory contains a practical and runnable [clap-based example](./examples/clap/), as well
as several other crates which are actually used for integration testing.

Here is the output for the Watchexec CLI:

```plain
watchexec 1.21.1 (5026793 2023-03-05)
commit-hash: 5026793a12ff895edf2dafb92111e7bd1767650e
commit-date: 2023-03-05
build-date: 2023-03-05
release: 1.21.1
features:
```

For comparison, here's `rustc -Vv`:

```plain
rustc 1.67.1 (d5a82bbd2 2023-02-07)
binary: rustc
commit-hash: d5a82bbd26e1ad8b7401f6a718a9c57c96905483
commit-date: 2023-02-07
host: x86_64-unknown-linux-gnu
release: 1.67.1
LLVM version: 15.0.6
```
