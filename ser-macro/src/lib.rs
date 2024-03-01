use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, DeriveInput, Error, ItemEnum, ItemStruct, Meta, Token,
};

struct PacketParams(syn::LitInt, syn::Ident);

impl Parse for PacketParams {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let type1 = input.parse()?;
        input.parse::<Token![,]>()?;
        let type2 = input.parse()?;
        Ok(PacketParams(type1, type2))
    }
}

// https://doc.rust-lang.org/reference/procedural-macros.html#attribute-macros
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

    let ident = input.ident;

    // let tokens = packet_attr.to_token_stream();
    let Meta::List(meta) = &packet_attr.meta else {
        let error = Error::new_spanned(
            &packet_attr.meta,
            "invalid `#[packet]` attribute. Example: `#[packet(0, Handshake)]`",
        );
        return TokenStream::from(error.to_compile_error());
    };

    let tokens = meta.tokens.clone();

    let Ok(PacketParams(id, kind)) = syn::parse(tokens.into()) else {
        let error = Error::new_spanned(
            meta,
            "invalid `#[packet]` attribute. Example: `#[packet(0, Handshake)]`",
        );
        return TokenStream::from(error.to_compile_error());
    };

    let expanded = quote! {
        impl ::ser::Packet for #ident {
            const ID: i32 = #id;
            const STATE: ser::types::PacketState = ser::types::PacketState::#kind;
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(Writable)]
pub fn writable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);

    let name = input.ident;

    let idents: Vec<_> = input.fields.iter().map(|x| x.ident.as_ref().unwrap()).collect();

    let expanded = quote! {
        impl ::ser::Writable for #name {
            fn write(self, writer: &mut impl ::std::io::Write) -> ::std::io::Result<()> {
                // todo: make sure to make sure all fields are ::ser::Writable
                #(self.#idents.write(writer)?;)*
                Ok(())
            }
            
            async fn write_async(self, writer: &mut (impl ::tokio::io::AsyncWrite + ::std::marker::Unpin)) -> ::std::io::Result<()> {
                // todo: make sure to make sure all fields are ::ser::Writable
                #(self.#idents.write_async(writer).await?;)*
                Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(Readable)]
pub fn readable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);

    let name = input.ident;

    let idents: Vec<_> = input.fields.iter().map(|x| x.ident.as_ref().unwrap()).collect();
    let types: Vec<_> = input.fields.iter().map(|x| &x.ty).collect();

    let expanded = quote! {
        impl ::ser::Readable for #name {
            fn read(reader: &mut impl ::std::io::BufRead) -> ::std::io::Result<Self> {
                Ok(#name {
                    #(#idents: <#types as ::ser::Readable>::read(reader)?),*
                })
            }
            
            async fn read_async(reader: &mut (impl ::tokio::io::AsyncBufRead + ::std::marker::Unpin)) -> ::std::io::Result<Self> {
                Ok(#name {
                    #(#idents: <#types as ::ser::Readable>::read_async(reader).await?),*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(EnumWritable)]
pub fn enum_writable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);

    let name = input.ident;

    let expanded = quote! {
        impl ByteWritable for #name {
            fn write_to_bytes(self, writer: &mut ByteWriter) {
                let v = self as i32;
                writer.write(VarInt(v));
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(EnumReadable)]
pub fn enum_readable_count(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);

    let name = input.ident;

    let idents: Vec<_> = input.variants.iter().map(|x| x.ident.clone()).collect();
    
    // for instance if we have enum Foo { A = 3, B = 5,
    // C = 7}, then the discriminants will be 3, 5, 7 else default to idx
    // let discriminants = // todo
    let discriminants: Vec<_> = input.variants.iter().enumerate().map(|(idx, v)| {
        // Attempt to find an explicit discriminant
        match &v.discriminant {
            Some((_, expr)) => quote! { #expr },
            None => {
                let idx = idx as u32; // Assuming u32 for simplicity; adjust as needed
                quote! { #idx }
            }
        }
    }).collect();

    let expanded = quote! {
        impl ser::Readable for #name {
            fn read(byte_reader: &mut impl ::std::io::BufRead) -> ::std::io::Result<Self> {
                let VarInt(inner) = VarInt::read(byte_reader)?;

                match inner {
                    #(#discriminants => Ok(#name::#idents)),*,
                    _ => ::std::result::Result::Err(::std::io::Error::new(::std::io::ErrorKind::InvalidData, "Invalid enum discriminant"))
                }
            }
            
            async fn read_async(byte_reader: &mut (impl ::tokio::io::AsyncBufRead + ::std::marker::Unpin)) -> ::std::io::Result<Self> {
                let VarInt(inner) = VarInt::read_async(byte_reader).await?;

                match inner {
                    #(#discriminants => Ok(#name::#idents)),*,
                    _ => ::std::result::Result::Err(::std::io::Error::new(::std::io::ErrorKind::InvalidData, "Invalid enum discriminant"))
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
