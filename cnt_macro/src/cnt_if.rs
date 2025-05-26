use crate::construct::{CounterKind, static_variable};
use crate::input_args::ExprAndNameArgs;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse2;

pub(crate) fn inner(args: TokenStream) -> syn::Result<TokenStream> {
    let input = parse2::<ExprAndNameArgs>(args)?;
    let expr = &input.expr;
    let counter_name = &input.name.value();
    let increment_fn = match &input.ty {
        Some(ty) => {
            if ty == "u32" {
                Ident::new("increment_u32", Span::call_site())
            } else if ty == "u64" {
                Ident::new("increment_u64", Span::call_site())
            } else {
                return Err(syn::Error::new(
                    ty.span(),
                    "only `u32` and `u64` counters are supported.",
                ));
            }
        }
        None => Ident::new("increment_u32", Span::call_site()),
    };

    let data = format!(
        "{counter_name},{}",
        input.ty.map(|t| t.to_string()).unwrap_or("u32".into())
    );
    let counter_idx = static_variable(CounterKind::Info, data.as_str());
    let tokens = quote! {
        if #expr {
            let counter_idx = #counter_idx;
            unsafe { cnt::#increment_fn(counter_idx); };
        }
    };
    // eprintln!("{tokens}");
    Ok(tokens)
}
