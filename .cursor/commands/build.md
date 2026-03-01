# build mostro-cli using coding standards


## Overview

Build and test mostro-cli, fixing all errors reported by cargo and clippy.

## Steps

- execute cargo fmt --all
- execute cargo clippy --all-targets --all-features
- execute cargo test
- execute cargo build
