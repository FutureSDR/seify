extern crate proc_macro;
mod seify_drivers;
use seify_drivers::*;

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::{Expr, ItemEnum, parse_macro_input};

/// Attribute macro to generate a `probe` method for enums annotated with `#[seify_drivers]`.
#[proc_macro_attribute]
pub fn seify_drivers(attr: TokenStream, item: TokenStream) -> TokenStream {
    match seify_drivers_impl(attr.into(), item.into()) {
        Ok(smth) => smth.into(),
        Err(err) => err.into_compile_error().into(),
    }
}
