# Contribution guidebook


This is a fairly free-form project, with low contribution traffic. It is mostly passively
maintained, with occasional bouts of more active developing, usually along a theme. Most of the
maintainer activity is triaging and managing issues, and loosely planning/grouping feature requests
and non-critical bugs to fix into these themes, represented as milestones.

Maintainers:

- Félix Saparelli (@passcod) (active)
- Matt Green (@mattgreen) (original author, more passive now)

Currently the project is in a long v1 phase, with a "sometime in the future" second phase, which at
the moment I envision as something with a stronger semver focus, and a major version number that may
increase more often. Possibly the project would be split in two or more crates, to facilitate this.

So, semver. Watchexec remains semver-compatible... on the CLI interface only. The library interface
is explicitly _not_ considered as part of breaking changes, and even _patch_ version number changes
may actually involve API breakage at the library interface.

Additionally, it is to be considered unwise to rely on the _output_ of the CLI as an interface. The
semver aspects are fairly loose but focus on input (flags, options, arguments, env), behaviour, and
the calling environment of the sub process.

Contributions are accepted for literally everything. There are a few anti goals:

- Calling watchexec is to be a **simple** exercise that remains intuitive. As a specific point, it
  should not involve any piping or require xargs.

- Watchexec will not be tied to any particular ecosystem or language. Projects that themselves use
  watchexec (the library) can be focused on a particular domain (for example Cargo Watch for Rust),
  but watchexec itself will remain generic, usable for any purpose.


## PR etiquette

- Maintainers are busy or may not have the bandwidth, be patient.
- Do _not_ change the version number in the PR.
- Do _not_ change Cargo.toml or other project metadata, unless specifically asked for, or if that's
  the point of the PR (like adding a crates.io category).

Apart from that, welcome and thank you for your time!


## Releasing

A release goes through these steps:

1. Opening a draft release. Before even merging anything, a draft (only visible privately) release
   is made. These are a github feature and only visible to maintainers. Name the release... that can
   be descriptive or whimsical or both. Release title template is `{version without v}: {title}`.

2. Adding each change to the draft release. The releases pages on github serves as a changelog, so
   this is worth getting right. One sentence per change, focusing on what it is, what it adds, what
   it changes, if any. Add a link or PR/issue number if relevant. For example:

   > - #160 :warning: Stop initialising the logger in the library code. Downstream users will need
   >   to initialise their own logger if they want debug/warn output.

3. Merging the PRs. Merge commits are preferred over rebase or squash.

4. Cleaning up the code and documentation if needed. For example a PR that adds a flag may not have
   also added the corresponding completions, manpage entries, readme entries. Or two PRs may
   conflict slightly or do the same thing twice, in which case harmonising things is required here.

5. Run `cargo fmt`, `cargo test`, `cargo clippy`, `bin/manpage`. Commit the result, if any.
   CI will also run, wait for that. In the meantime:

6. Run through related issues to the PRs and close them if that wasn't done automatically. Or if the
   PRs only fixed a problem partially, chime in to mention that, and to restate what remains to fix.

7. "Real" test the new code. If new options were added, test those.

8. Check for any dependency updates with `cargo outdated -R`.

9. Run `bin/version 1.2.3` where `1.2.3` is the new version number. This will tag and push,
   triggering the GitHub Action for releases.

10. Wait for all builds to complete, then attach the draft release to the tag, and publish it.

11. Run the `cargo publish`.

12. Announce the release.

---
vim: tw=100
