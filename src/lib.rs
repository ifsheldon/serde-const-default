//! Serde defaults backed by const expressions.
//!
//! Serde's `#[serde(default = "...")]` field attribute expects a path to a
//! zero-argument function. This crate lets fields name the expression directly
//! and rewrites that expression into the helper function Serde already knows
//! how to call.
//!
//! Use [`serde_const_default`] on the struct, then mark fields with either
//! `#[const_default = EXPR]` or `#[const_default_from(EXPR)]`.
//!
//! ```
//! use serde::Deserialize;
//! use serde_const_default::serde_const_default;
//!
//! const SOME_VALUE: u8 = 7;
//! const DEFAULT_NAME: &str = "anonymous";
//!
//! #[serde_const_default]
//! #[derive(Deserialize)]
//! struct Config {
//!     #[const_default = SOME_VALUE]
//!     count: u8,
//!
//!     #[const_default_from(DEFAULT_NAME)]
//!     name: String,
//! }
//!
//! let value: Config = serde_json::from_str("{}").unwrap();
//! assert_eq!(value.count, 7);
//! assert_eq!(value.name, "anonymous");
//! ```

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Error, Expr, Field, Fields, ItemStruct, LitStr, Meta, Token, parse_macro_input,
    parse_quote,
};

/// Enables const-expression defaults for fields on a named struct.
///
/// The macro consumes field attributes using the following forms:
///
/// - `#[const_default = EXPR]`, which generates a `const fn` that returns
///   `EXPR` directly.
/// - `#[const_default_from(EXPR)]`, which generates a default function that
///   returns `::core::convert::From::from(EXPR)`.
///
/// The generated helper functions are wired back into Serde with
/// `#[serde(default = "...")]`, so the struct should also derive or implement
/// Serde deserialization in the usual way.
///
/// ```
/// use serde::Deserialize;
/// use serde_const_default::serde_const_default;
///
/// const PORT: u16 = 8080;
///
/// #[serde_const_default]
/// #[derive(Deserialize)]
/// struct Settings {
///     #[const_default = PORT]
///     port: u16,
/// }
///
/// let settings: Settings = serde_json::from_str("{}").unwrap();
/// assert_eq!(settings.port, 8080);
/// ```
///
/// Use `const_default_from` when the expression's type needs a `From`
/// conversion into the field type:
///
/// ```
/// use serde::Deserialize;
/// use serde_const_default::serde_const_default;
///
/// const DEFAULT_NAME: &str = "guest";
///
/// #[serde_const_default]
/// #[derive(Deserialize)]
/// struct Settings {
///     #[const_default_from(DEFAULT_NAME)]
///     name: String,
/// }
///
/// let settings: Settings = serde_json::from_str("{}").unwrap();
/// assert_eq!(settings.name, "guest");
/// ```
#[proc_macro_attribute]
pub fn serde_const_default(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new_spanned(
            proc_macro2::TokenStream::from(attr),
            "`serde_const_default` does not accept arguments",
        )
        .to_compile_error()
        .into();
    }

    let item = parse_macro_input!(item as ItemStruct);
    expand_serde_const_default(item)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_serde_const_default(mut item: ItemStruct) -> Result<proc_macro2::TokenStream, Error> {
    let mut helpers = Vec::new();
    let struct_ident = item.ident.clone();

    match &mut item.fields {
        Fields::Named(fields) => {
            for (index, field) in fields.named.iter_mut().enumerate() {
                if let Some(helper) = rewrite_field(&struct_ident, index, field)? {
                    helpers.push(helper);
                }
            }
        }
        Fields::Unnamed(fields) => {
            for field in &fields.unnamed {
                if has_const_default_attr(field) {
                    // Helper names are derived from field identifiers, so keep
                    // tuple structs unsupported until the API has a clear
                    // naming rule for generated defaults.
                    return Err(Error::new_spanned(
                        field,
                        "`serde_const_default` only supports named fields",
                    ));
                }
            }
        }
        Fields::Unit => {}
    }

    Ok(quote! {
        #(#helpers)*
        #item
    })
}

fn rewrite_field(
    struct_ident: &syn::Ident,
    index: usize,
    field: &mut Field,
) -> Result<Option<proc_macro2::TokenStream>, Error> {
    let Some(default) = take_const_default_attr(&mut field.attrs)? else {
        return Ok(None);
    };

    if let Some(attr) = field.attrs.iter().find(|attr| serde_attr_has_default(attr)) {
        // Serde accepts only one default source. Rejecting this explicitly
        // keeps the user's chosen default from depending on attribute order.
        return Err(Error::new_spanned(
            attr,
            "`const_default` cannot be combined with `#[serde(default)]`",
        ));
    }

    let Some(field_ident) = field.ident.as_ref() else {
        return Err(Error::new_spanned(
            &*field,
            "`serde_const_default` only supports named fields",
        ));
    };
    let helper_ident = format_ident!(
        "__serde_const_default_{}_{}_{}",
        struct_ident,
        field_ident,
        index
    );
    let helper_name = LitStr::new(&helper_ident.to_string(), helper_ident.span());
    let ty = &field.ty;
    let expr = default.expr;
    let (function_qualifier, body) = match default.kind {
        DefaultKind::Plain => (quote! { const }, quote! { #expr }),
        DefaultKind::From => (quote! {}, quote! { ::core::convert::From::from(#expr) }),
    };

    // Serde's derive macro reads default helpers through a string literal path,
    // so the custom field attribute is replaced with the equivalent Serde
    // attribute before the struct is emitted.
    field.attrs.push(parse_quote! {
        #[serde(default = #helper_name)]
    });

    Ok(Some(quote! {
        #[allow(non_snake_case)]
        #function_qualifier fn #helper_ident() -> #ty {
            #body
        }
    }))
}

fn take_const_default_attr(attrs: &mut Vec<Attribute>) -> Result<Option<ConstDefault>, Error> {
    let mut default = None;
    let mut error = None::<Error>;
    let mut retained = Vec::with_capacity(attrs.len());

    for attr in attrs.drain(..) {
        // Remove our custom attributes from the field. If they survive the
        // expansion, rustc validates them as ordinary attributes and rejects
        // useful expressions such as `#[const_default = SOME_CONST]`.
        match parse_const_default_attr(&attr)? {
            Some(parsed) => {
                if default.is_some() {
                    let duplicate_error = Error::new_spanned(
                        attr,
                        "only one const default attribute is allowed per field",
                    );

                    if let Some(error) = &mut error {
                        error.combine(duplicate_error);
                    } else {
                        error = Some(duplicate_error);
                    }
                } else {
                    default = Some(parsed);
                }
            }
            None => retained.push(attr),
        }
    }

    *attrs = retained;

    if let Some(error) = error {
        Err(error)
    } else {
        Ok(default)
    }
}

fn parse_const_default_attr(attr: &Attribute) -> Result<Option<ConstDefault>, Error> {
    if attr.path().is_ident("const_default") {
        let Meta::NameValue(name_value) = &attr.meta else {
            return Err(Error::new_spanned(
                attr,
                "expected `#[const_default = EXPR]`",
            ));
        };

        return Ok(Some(ConstDefault {
            kind: DefaultKind::Plain,
            expr: name_value.value.clone(),
        }));
    }

    if attr.path().is_ident("const_default_from") {
        return Ok(Some(ConstDefault {
            kind: DefaultKind::From,
            expr: attr.parse_args::<Expr>()?,
        }));
    }

    Ok(None)
}

/// Checks tuple-struct fields without fully parsing their defaults.
fn has_const_default_attr(field: &Field) -> bool {
    field.attrs.iter().any(|attr| {
        attr.path().is_ident("const_default") || attr.path().is_ident("const_default_from")
    })
}

/// Returns true when a Serde field attribute already configures a default.
fn serde_attr_has_default(attr: &Attribute) -> bool {
    if !attr.path().is_ident("serde") {
        return false;
    }

    let Meta::List(list) = &attr.meta else {
        return false;
    };

    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map(|metas| {
            metas.iter().any(|meta| match meta {
                Meta::Path(path) => path.is_ident("default"),
                Meta::NameValue(name_value) => name_value.path.is_ident("default"),
                Meta::List(list) => list.path.is_ident("default"),
            })
        })
        .unwrap_or(false)
}

/// Parsed representation of a field-level const default attribute.
struct ConstDefault {
    kind: DefaultKind,
    expr: Expr,
}

/// Selects whether the generated helper is a direct const default or a runtime
/// conversion default.
enum DefaultKind {
    Plain,
    From,
}
