use proc_macro::TokenStream;

mod cnt_if;
mod construct;
mod input_args;
mod symbol;

#[proc_macro]
pub fn cnt_if(args: TokenStream) -> TokenStream {
    match cnt_if::inner(args.into()) {
        Ok(result) => result.into(),
        Err(e) => e.into_compile_error().into(),
    }
}
