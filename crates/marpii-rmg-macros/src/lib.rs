use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

///Derives a compute pipeline from the given shader-source.
///
/// Supported attributes:
///
/// - `push_constant`: signals that this is a push constant
/// - `pc_handle = xy`: signals that the tagged resource is referenced in the push constant as `xy`. Will automatically update the field `xy` of the `#[push_constant]` marked constant with the runtime [ResourceHandle](marpii_rmg_shared::ResourceHandle).
///
#[proc_macro_derive(Compute, attributes(push_constant, pc_handle))]
pub fn compute_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    quote!().into()
}

///Derives a dynamic-rendering based graphics pipeline from the given shader-source.
#[proc_macro_derive(DynamicRendering)]
pub fn dynamic_rendering(input: TokenStream) -> TokenStream {
    todo!()
}
