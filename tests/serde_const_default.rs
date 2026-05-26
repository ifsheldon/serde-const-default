use serde::Deserialize;
use serde_const_default::serde_const_default;
use std::sync::LazyLock;

const SOME_VALUE: u8 = 7;
const DEFAULT_NAME: &str = "anonymous";
static LAZY_NAME: LazyLock<String> = LazyLock::new(|| "lazy".to_string());

#[serde_const_default]
#[derive(Debug, Deserialize, PartialEq)]
struct PlainDefault {
    #[const_default = SOME_VALUE]
    count: u8,
}

#[serde_const_default]
#[derive(Debug, Deserialize, PartialEq)]
struct ConvertedDefault {
    #[const_default_from(DEFAULT_NAME)]
    name: String,
}

#[serde_const_default]
#[derive(Debug, Deserialize, PartialEq)]
struct LazyDefault {
    #[const_default_from(LAZY_NAME.clone())]
    name: String,
}

const GENERATED_DIRECT_DEFAULT: u8 = __serde_const_default_PlainDefault_count_0();

#[test]
fn const_default_uses_expression_directly() {
    let value: PlainDefault = serde_json::from_str("{}").unwrap();

    assert_eq!(value.count, SOME_VALUE);
}

#[test]
fn present_field_overrides_const_default() {
    let value: PlainDefault = serde_json::from_str(r#"{"count": 3}"#).unwrap();

    assert_eq!(value.count, 3);
}

#[test]
fn const_default_from_converts_expression() {
    let value: ConvertedDefault = serde_json::from_str("{}").unwrap();

    assert_eq!(value.name, "anonymous");
}

#[test]
fn const_default_generates_const_fn() {
    assert_eq!(GENERATED_DIRECT_DEFAULT, SOME_VALUE);
}

#[test]
fn const_default_from_supports_explicit_lazy_clones() {
    let value: LazyDefault = serde_json::from_str("{}").unwrap();

    assert_eq!(value.name, "lazy");
}
