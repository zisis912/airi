use std::sync::LazyLock;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde_json::Value;
use syn::{
    Data, DeriveInput, Ident, LitInt, LitStr, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

// TODO: use darling at some point

const ALPHABET: [&str; 10] = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];

#[proc_macro_derive(Serializable, attributes(enum_info, enum_idx, bitfields))]
pub fn derive_serializable(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let read_from: TokenStream;
    let write_to: TokenStream;

    let name = input.ident;
    let name_str = name.to_string();

    match input.data {
        Data::Struct(s) => {
            let mut field_reads: Vec<TokenStream> = Vec::new();
            let mut field_writes: Vec<TokenStream> = Vec::new();

            if let Some(bitfields) = input.attrs.iter().find(|attr| {
                let Ok(metalist) = attr.meta.require_list() else {
                    return false;
                };
                metalist.path.is_ident("bitfields")
            }) {
                let Bitfields { ty } = bitfields.parse_args().unwrap();
                match &s.fields {
                    syn::Fields::Named(f) => {
                        for (i, field) in f.named.iter().enumerate() {
                            let name = &field.ident;

                            match &field.ty {
                                Type::Path(ty_path) => {
                                    if ty_path.path.segments.iter().next().unwrap().ident != "bool"
                                    {
                                        panic!("bitfield only works with bool")
                                    }
                                }
                                _ => panic!("bitfield only works with bool"),
                            }

                            field_reads.push(quote!( #name: val & (1 << #i) != 0 ));
                            field_writes.push(quote!( val |= (self.#name as #ty) << #i; ));
                        }
                    }
                    _ => unimplemented!(),
                };

                read_from = quote! {
                    let val = #ty::read_from(buf)?;
                    Ok(Self {
                        #(#field_reads),*
                    })
                };

                write_to = quote! {
                    let mut val: #ty = 0;
                    #(#field_writes)*
                    val.write_to(buf)?;
                    Ok(())
                };
            } else {
                match &s.fields {
                    syn::Fields::Named(f) => {
                        for field in &f.named {
                            let name = &field.ident;
                            field_reads.push(quote!( #name: Serializable::read_from(buf)? ));
                            field_writes.push(quote!( self.#name.write_to(buf)?; ));
                        }

                        read_from = quote! {
                            Ok(Self {
                                #(#field_reads),*
                            })
                        };
                    }
                    syn::Fields::Unnamed(f) => {
                        for (i, _field) in f.unnamed.iter().enumerate() {
                            let idx = syn::Index::from(i);

                            field_reads.push(quote!(Serializable::read_from(buf)?));
                            field_writes.push(quote!( self.#idx.write_to(buf)?; ));
                        }

                        read_from = quote! {
                            Ok(Self( #(#field_reads),* ))
                        };
                    }
                    syn::Fields::Unit => {
                        read_from = quote! {
                            Ok(Self)
                        }
                    }
                };

                write_to = quote! {
                    #(#field_writes)*
                    Ok(())
                }
            }
        }
        Data::Enum(e) => {
            // REQUIRE ENUM INFO ATTR
            let Some(enum_info_attr) = input.attrs.iter().find(|attr| {
                let Ok(metalist) = attr.meta.require_list() else {
                    return false;
                };
                metalist.path.is_ident("enum_info")
            }) else {
                panic!("enum_info attribute missing")
            };

            let EnumInfo { ty } = enum_info_attr.parse_args().expect("enum info parse failed");

            let mut num_to_variant: Vec<TokenStream> = Vec::new();
            let mut variant_to_num: Vec<TokenStream> = Vec::new();

            let mut extra_offset = 0;

            // pretty much enumerate but with starting index
            for (mut idx, variant) in e.variants.iter().enumerate() {
                let name = &variant.ident;

                // increase offset if needed
                if let Some(new_idx) = variant.attrs.iter().find_map(|attr| {
                    if attr.path().is_ident("enum_idx") {
                        let value: LitInt = attr.parse_args().expect("expected int");
                        return Some(value.base10_parse::<usize>().unwrap());
                    }
                    Option::<usize>::None
                }) {
                    if new_idx > idx {
                        extra_offset = new_idx - idx;
                    } else {
                        panic!("cant set an index lower than what it would normally be");
                    }
                }

                idx += extra_offset;

                match &variant.fields {
                    syn::Fields::Named(f) => {
                        let mut field_reads: Vec<TokenStream> = Vec::new();
                        let mut field_writes: Vec<TokenStream> = Vec::new();

                        let mut field_names: Vec<Ident> = Vec::new();

                        for field in &f.named {
                            let name = &field.ident;
                            field_names.push(name.clone().unwrap());

                            field_reads.push(quote!(#name: Serializable::read_from(buf)?));
                            field_writes.push(quote!( #name.write_to(buf)?; ));
                        }
                        num_to_variant.push(quote!( #idx => Self::#name{ #(#field_reads),* } ));
                        variant_to_num.push(quote!(
                            Self::#name {#(#field_names),*} => {
                            #ty::from_len(#idx).write_to(buf)?;
                            #(#field_writes)*
                            }
                        ));
                    }
                    syn::Fields::Unnamed(f) => {
                        let mut field_reads: Vec<TokenStream> = Vec::new();
                        let mut field_writes: Vec<TokenStream> = Vec::new();

                        let mut field_names: Vec<Ident> = Vec::new();

                        for (i, _field) in f.unnamed.iter().enumerate() {
                            // let ident = &field.ident;
                            let field_name = format_ident!("{}", ALPHABET[i]);
                            field_names.push(field_name.clone());

                            field_reads.push(quote!(Serializable::read_from(buf)?));
                            field_writes.push(quote!( #field_name.write_to(buf)?; ));
                        }

                        num_to_variant.push(quote!( #idx => Self::#name( #(#field_reads),* ) ));
                        variant_to_num.push(quote!(
                            Self::#name(#(#field_names),*) => {
                            #ty::from_len(#idx).write_to(buf)?;
                            #(#field_writes)*
                            }
                        ));
                    }
                    syn::Fields::Unit => {
                        num_to_variant.push(quote!(#idx => Self::#name));
                        variant_to_num
                            .push(quote!(Self::#name => #ty::from_len(#idx).write_to(buf)?));
                    }
                };
            }

            read_from = quote! {
                Ok(match <#ty>::read_from(buf)?.into_len() {
                    #(#num_to_variant,)*
                    x @ _ => return Err(crate::ReadingError::Message(format!("invalid {} enum index: {}",#name_str,x)))
                })
            };

            write_to = quote! {
                match self {
                    #(#variant_to_num,)*
                };
                Ok(())
            }
        }
        Data::Union(_u) => {
            unimplemented!()
        }
    };

    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    quote! {
        impl #impl_generics Serializable for #name #type_generics #where_clause {
            fn read_from<R: std::io::Read>(buf: &mut R) -> Result<Self, crate::ReadingError> {
                #read_from
            }
            fn write_to<W: std::io::Write>(&self, buf: &mut W) -> Result<(), crate::WritingError> {
                #write_to
            }
        }
    }
    .into()
}

struct EnumInfo {
    ty: Type,
    // start_idx: i32,
}

impl Parse for EnumInfo {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<EnumInfo> {
        let ty = input.parse()?;
        // input.parse::<Token![,]>()?;
        // let start_idx = input.parse::<LitInt>()?.base10_parse()?;
        Ok(EnumInfo { ty })
    }
}

struct Bitfields {
    ty: Type,
}

impl Parse for Bitfields {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty = input.parse()?;
        Ok(Bitfields { ty })
    }
}

// use crate::registry::{BLOCK_STATE_REGISTRY, PACKET_REGISTRY, REGISTRIES};
static PACKET_REGISTRY: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/packets.json"))
        .expect("Could not parse packets.json registry.")
});

#[proc_macro]
pub fn get_entry(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let PacketLookupInput {
        state,
        dir,
        packet_name,
    } = parse_macro_input!(input as PacketLookupInput);

    let direction = match dir.as_str() {
        "Clientbound" => "clientbound",
        "Serverbound" => "serverbound",
        _ => panic!("invalid packet direction"),
    }
    .to_owned();

    let id: i32 =
        PACKET_REGISTRY[state][direction]["minecraft:".to_owned() + &packet_name]["protocol_id"]
            .as_i64()
            .unwrap()
            .try_into()
            .unwrap();

    quote! {#id}.into()
}

struct PacketLookupInput {
    state: String,
    dir: String,
    packet_name: String,
}

impl Parse for PacketLookupInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let state = input.parse::<Ident>()?.to_string();
        input.parse::<Token![,]>()?;
        let dir = input.parse::<Ident>()?.to_string();
        input.parse::<Token![,]>()?;
        let packet_name = input.parse::<LitStr>()?.value();
        Ok(PacketLookupInput {
            state,
            dir,
            packet_name,
        })
    }
}

#[proc_macro]
pub fn generate_blocks(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    airi_codegen::blocks::build().into()
}
