use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Lit, Error};

#[proc_macro_derive(CommandPermission, attributes(command_permission))]
pub fn derive_command_permission(input: TokenStream) -> TokenStream {
    // Parse the input as a DeriveInput (struct or enum)
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone(); // Clone the Ident to prevent moving

    // Extract the group from the `#[command_permission(group = "Admin")]` attribute
    let mut group = None;
    for attr in input.attrs.iter() {
        if attr.path().is_ident("command_permission") {
            if let Err(err) = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("group") {
                    if let Ok(Lit::Str(lit)) = meta.value()?.parse::<Lit>() {
                        group = Some(lit.value());
                    }
                }
                Ok(())
            }) {
                return Error::new_spanned(attr, format!("Failed to parse attribute: {}", err))
                    .to_compile_error()
                    .into();
            }
        }
    }

    let group = match group {
        Some(g) => g,
        None => {
            return Error::new_spanned(
                input,
                "Missing required `#[command_permission(group = \"<GroupName>\")]` attribute.",
            )
            .to_compile_error()
            .into();
        }
    };

    // Generate the trait implementation
    let expanded = quote! {
        impl CommandPermission for #name {
            fn has_required_permission(&self, user_group: hyperion_permission::Group) -> bool {
                use hyperion_permission::Group::*;
                match (#group, user_group) {
                    ("Banned", Banned) => true,
                    ("Banned", _) => false,
                    ("Normal", Normal | Moderator | Admin) => true,
                    ("Moderator", Moderator | Admin) => true,
                    ("Admin", Admin) => true,
                    _ => false,
                }
            }
        }
    };

    TokenStream::from(expanded)
}
