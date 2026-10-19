<!-- Thanks for contributing! Please fill in the sections below. -->

## What this changes

<!-- One or two sentences. What is different after this pull request? -->

## Why

<!-- Link the issue it closes, e.g. "Closes #42". If there is no issue, say what problem
     this solves. -->

## How it was tested

<!-- What did you run to check it? If the change touches the cleanup engine, the lock
     file, or the quarantine pass, describe how you verified it is safe. -->

- [ ] `cargo test`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo fmt --check`
- [ ] `cargo run -q -- audit plist config/com.USER.mac-setup.plist.template`

## Safety

<!-- If the change touches any of the invariants listed in CONTRIBUTING.md, call it out
     explicitly and explain how the regression is covered. Otherwise say "No change to
     the safety invariants." -->

- [ ] This change does not break any of the safety invariants in `CONTRIBUTING.md`.

## Checklist

- [ ] The diff contains only what this pull request is about.
- [ ] Tests were added or updated where the behaviour changed.
- [ ] Documentation was updated if a command, flag, or configuration value changed.
