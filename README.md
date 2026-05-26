# serde-const-default

Use const expressions as Serde defaults.

## Install
```shell
cargo add serde-const-default
```

Or, add it to `Cargo.toml`:

```toml
[dependencies]
serde-const-default = "0.1"
```

## Usage & Examples

```rust
use serde::Deserialize;
use serde_const_default::serde_const_default;

const SOME_VALUE: u8 = 7;
const DEFAULT_NAME: &str = "anonymous";

#[serde_const_default]
#[derive(Deserialize)]
struct Config {
    #[const_default = SOME_VALUE]
    count: u8,

    #[const_default_from(DEFAULT_NAME)]
    name: String,
}
```

`#[const_default = EXPR]` generates a Serde default function that returns
`EXPR` directly:

```rust
fn generated_default() -> FieldType {
    EXPR
}
```

`#[const_default_from(EXPR)]` generates a Serde default function that converts
the expression into the field type with `From::from`:

```rust
fn generated_default() -> FieldType {
    ::core::convert::From::from(EXPR)
}
```

Use `const_default_from` when the expression is not already the field type, for
example a `&'static str` default for a `String` field.

Lazy defaults should be accessed explicitly:

```rust
use serde::Deserialize;
use std::sync::LazyLock;
use serde_const_default::serde_const_default;

static DEFAULT_NAME: LazyLock<String> = LazyLock::new(|| "lazy".to_string());

#[serde_const_default]
#[derive(Deserialize)]
struct Config {
    #[const_default = DEFAULT_NAME.clone()]
    name: String,
}
```

The macro supports named structs. A field cannot use both `const_default` and
Serde's own `#[serde(default)]` attribute.
