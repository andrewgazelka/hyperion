use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, parse_quote, Data, DeriveInput, Error, GenericParam, Generics, ItemEnum,
    Meta,
};

#[proc_macro_derive(Packet, attributes(packet))]
pub fn packet(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let Some(packet_attr) = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("packet"))
    else {
        let error = Error::new_spanned(
            &input.ident,
            "missing `#[packet]` attribute. Example: `#[packet(0)]`",
        );
        return TokenStream::from(error.to_compile_error());
    };

    let ident = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let Meta::List(meta) = &packet_attr.meta else {
        let error = Error::new_spanned(
            &packet_attr.meta,
            "invalid `#[packet]` attribute. Example: `#[packet(0)]`",
        );
        return TokenStream::from(error.to_compile_error());
    };

    let tokens = meta.tokens.clone();

    // get literal
    let Ok(id) = syn::parse2::<syn::LitInt>(tokens) else {
        let error = Error::new_spanned(
            meta,
            "invalid `#[packet]` attribute. Example: `#[packet(0)]`",
        );
        return TokenStream::from(error.to_compile_error());
    };

    let expanded = quote! {
        impl #impl_generics ::ser::Packet for #ident #ty_generics #where_clause {
            const ID: i32 = #id;
            // const STATE: ser::types::PacketState = ser::types::PacketState::#kind;
            const NAME: &'static str = stringify!(#ident);
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(Writable)]
pub fn writable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;
    let generics = input.generics;
    let (generics, where_clause) = extend_generics_and_create_where_clause(&generics);

    // Extracting field identifiers and ensuring that the struct can actually be used with this
    // macro.
    let idents: Vec<_> = match input.data {
        syn::Data::Struct(data_struct) => data_struct
            .fields
            .iter()
            .filter_map(|f| f.ident.clone())
            .collect(),
        _ => return TokenStream::new(), // Early return if not struct
    };

    let expanded = quote! {
        impl #generics ::ser::Writable for #name #generics #where_clause {
            fn write(&self, writer: &mut impl ::std::io::Write) -> anyhow::Result<()> {
                #(self.#idents.write(writer)?;)*
                Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}

// lifetime if needed, and to create a where clause that bounds all fields by the
// specified lifetime. It returns a tuple of the possibly extended generics and
// the where clause.
fn extend_generics_and_create_where_clause(
    generics: &Generics,
) -> (Generics, proc_macro2::TokenStream) {
    let mut generics = generics.clone();
    let mut where_clause = generics.make_where_clause().predicates.clone();

    for param in &generics.params {
        if let GenericParam::Type(type_param) = param {
            // Assuming all types must implement the 'Readable' trait bound by a certain lifetime
            where_clause.push(parse_quote!(#type_param: ::ser::Readable<'a>));
        }
    }

    (generics, quote!(#where_clause))
}

#[proc_macro_derive(Readable)]
pub fn readable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;
    let generics = input.generics;
    let (generics, where_clause) = extend_generics_and_create_where_clause(&generics);

    // if generics is empty, then make it <'_>
    let readable_generics = if generics.params.is_empty() {
        quote! { <'_> }
    } else {
        quote! { #generics }
    };

    let slice_generics = {
        let lifetimes = generics.lifetimes().collect::<Vec<_>>();
        if !lifetimes.is_empty() {
            // Assume using the first lifetime if available
            let lifetime = lifetimes[0];
            quote! { &#lifetime }
        } else {
            // Default to '_ if no lifetimes are present
            quote! { &'_ }
        }
    };

    let Data::Struct(data) = input.data else {
        let error = Error::new_spanned(
            &name,
            "only structs are supported for the `#[derive(Readable)]` attribute",
        );
        return TokenStream::from(error.to_compile_error());
    };

    let idents: Vec<_> = data
        .fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap())
        .collect();

    let expanded = quote! {
        impl #generics ::ser::Readable #readable_generics for #name #generics where #where_clause {
            fn decode(r: &mut #slice_generics [u8]) -> ::anyhow::Result<Self> {
                Ok(Self {
                    #(#idents: ::ser::Readable::decode(r)?,)*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(EnumWritable)]
pub fn enum_writable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let expanded = quote! {
        impl ::ser::Writable for #name {
            fn write(&self, writer: &mut impl ::std::io::Write) -> anyhow::Result<()> {
                let v = *self as i32;
                let v = VarInt(v);
                v.write(writer)
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(EnumReadable)]
pub fn enum_readable_count(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let Data::Enum(data) = input.data else {
        let error = Error::new_spanned(
            &name,
            "only enums are supported for the `#[derive(EnumReadable)]` attribute",
        );
        return TokenStream::from(error.to_compile_error());
    };

    let idents: Vec<_> = data.variants.iter().map(|x| x.ident.clone()).collect();

    // for instance if we have enum Foo { A = 3, B = 5,
    // C = 7}, then the discriminants will be 3, 5, 7 else default to idx
    // let discriminants = // todo
    let discriminants: Vec<_> = data
        .variants
        .iter()
        .enumerate()
        .map(|(idx, v)| {
            // Attempt to find an explicit discriminant
            match &v.discriminant {
                Some((_, expr)) => quote! { #expr },
                None => {
                    let idx = idx as i32;
                    quote! { #idx }
                }
            }
        })
        .collect();

    let expanded = quote! {
        impl ::ser::Readable<'_> for #name {
            fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
                let ::ser::types::VarInt(inner) = ::ser::types::VarInt::decode(r)?;
                match inner {
                    #(#discriminants => Ok(#name::#idents),)*
                    _ => Err(anyhow::anyhow!("invalid discriminant"))
                }
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(AdtReadable)]
pub fn enum_readable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);

    let name = input.ident;

    // let mut discriminants =
    // input.variants.iter().map(|x|x.discriminant.clone().unwrap().1);

    let discriminants = input
        .variants
        .iter()
        .enumerate()
        .map(|(a, _)| proc_macro2::Literal::i32_unsuffixed(a as i32));

    let mut variants_ts = Vec::new();
    for variant in input.variants.clone() {
        let var_ident = variant.ident;
        let var_fields = variant.fields.iter().map(|x| x.ident.clone());
        let variant_ts = quote! {
            #name::#var_ident {
                #(#var_fields: byte_reader.read()),*
            }
        };
        variants_ts.push(variant_ts);
    }

    let expanded = quote! {
        impl ser::read::ByteReadable for #name {
            fn read_from_bytes(byte_reader: &mut ser::read::ByteReader) -> Self {
                let VarInt(inner) = byte_reader.read();

                let res = match inner {
                    #(#discriminants => Some(#variants_ts)),*,
                    _ => None
                };

                res.unwrap()
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(AdtWritable)]
pub fn adt_writable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);

    let name = input.ident;

    let idents: Vec<_> = input.variants.iter().map(|x| x.ident.clone()).collect();

    let discriminants = input
        .variants
        .iter()
        .enumerate()
        .map(|(a, _)| proc_macro2::Literal::i32_unsuffixed(a as i32));

    let mut variants_ts = Vec::new();
    for variant in input.variants.clone() {
        let var_ident = variant.ident;
        let var_fields: Vec<_> = variant
            .fields
            .iter()
            .map(|x| x.ident.clone().unwrap())
            .collect();
        let variant_ts = quote! {
            #name::#var_ident { #(#var_fields),* }=> {
                #(writer.write(#var_fields));*;
            }
        };
        variants_ts.push(variant_ts);
    }

    let expanded = quote! {
        impl ser::write::ByteWritable for #name {
            fn write_to_bytes(self, writer: &mut ser::write::ByteWriter) {

                let id = match self {
                    #(#name::#idents{..} => #discriminants),*,
                };

                let id = VarInt(id);

                writer.write(id);

                match self {
                    #(#variants_ts),*,
                };

            }
        }
    };

    TokenStream::from(expanded)
}
