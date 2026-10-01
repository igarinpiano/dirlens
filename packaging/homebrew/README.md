# Homebrew Formula

`dirlens.rb` is the Formula prepared for submission to `Homebrew/homebrew-core`.
The Formula is kept here as the upstream reference; the accepted copy will live
in Homebrew's `Formula/d/dirlens.rb`.

It builds the CLI from the Rust workspace under `rust/`, installs shell
completions and the generated man page, and runs a local functional test without
network access.

## Validate locally

From a checkout of `Homebrew/homebrew-core` containing this Formula:

```bash
HOMEBREW_NO_INSTALL_FROM_API=1 brew install --build-from-source dirlens
brew test dirlens
brew audit --strict --new --online dirlens
brew style --formula dirlens
brew lgtm --online
```

The source URL and SHA-256 correspond to the `v1.2.23` tag. When preparing a
later release, update both values and run the same validation before submitting
the Formula update.
