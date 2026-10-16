# Contributing to Turly

Thanks for helping out. Bug reports and pull requests are welcome.

## Setup

```sh
git clone https://github.com/mwithington/turly
cd turly
cargo run -- path/to/workspace
```

Before opening a PR:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

Snapshot tests use [`insta`](https://insta.rs). After an intended output change, run `cargo insta review` (or `INSTA_UPDATE=always cargo test`) and commit the updated `.snap` files.

## Commit convention

Commits and PR titles follow [Conventional Commits](https://www.conventionalcommits.org). [release-please](https://github.com/googleapis/release-please) reads them to pick the next version and write the changelog.

| Prefix | Effect |
| --- | --- |
| `feat:` | minor release |
| `fix:` | patch release |
| `feat!:` or a `BREAKING CHANGE:` footer | major release (minor while below 1.0) |
| `docs:`, `refactor:`, `test:`, `chore:`, `ci:` | no release |

## Releases

Merging to `main` keeps a release PR up to date. Merging that PR tags the release, builds binaries for macOS, Linux and Windows, attaches them to the GitHub release, and publishes to crates.io (needs the `CARGO_REGISTRY_TOKEN` repository secret).

## Dependencies

Renovate opens grouped update PRs. Minor and patch updates automerge once CI passes; major updates need a manual review.

## License

By contributing you agree that your work is dual-licensed under MIT and Apache-2.0, as in the rest of the project.
