[![Crates.io page](https://badgen.net/crates/v/ignore-files)](https://crates.io/crates/ignore-files)
[![API Docs](https://docs.rs/ignore-files/badge.svg)][docs]
[![Crate license: Apache 2.0](https://badgen.net/badge/license/Apache%202.0)][license]
[![CI status](https://github.com/watchexec/watchexec/actions/workflows/check.yml/badge.svg)](https://github.com/watchexec/watchexec/actions/workflows/check.yml)

# Ignore files

_Find, parse, and interpret ignore files._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license].
- Status: done.

## Supported ignore file formats

This crate supports parsing ignore files from various version control systems and tools:

- **Git** (`.gitignore`) - Full gitignore syntax including negation, wildcards, and path matching
- **Mercurial** (`.hgignore`) - Supports both glob and regex patterns with syntax prefixes
- **Bazaar** (`.bzrignore`) - Supports glob patterns, regex patterns with `RE:` prefix, and case-insensitive regex with `RE:(?i)` prefix

### Bazaar ignore patterns

The bazaar parser supports the following pattern types:

- **Glob patterns** (default): Standard shell-style wildcards (`*`, `?`, `[abc]`, `/**/`)
- **Regular expressions**: Prefixed with `RE:` (e.g., `RE:.*\.tmp$`)
- **Case-insensitive regex**: Prefixed with `RE:(?i)` (e.g., `RE:(?i)foo`)
- **Negation**: Prefix any pattern with `!` to whitelist it
- **Escaped characters**: Use `\!` to match literal exclamation marks

Example `.bzrignore` file:
```
# Build artifacts
*.o
*.so
!important.so

# Case insensitive image files
RE:(?i).*\.(jpg|png|gif)$

# Root directory only
./config
```

See the [bzr_parser example](examples/bzr_parser.rs) for more details.

[docs]: https://docs.rs/ignore-files
[license]: ../../LICENSE
