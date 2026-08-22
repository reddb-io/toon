use proc_macro::TokenStream;

#[proc_macro_derive(ToonRpcService, attributes(toon_rpc))]
pub fn derive_service(input: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    let name = &ast.ident;
    quote::quote! {
        impl reddb_io_toon_rpc::Service for #name {
            const NAME: &'static str = stringify!(#name);
        }
    }
    .into()
}
