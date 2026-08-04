use std::sync::LazyLock;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde_json::Value;
use syn::{
    Data, DeriveInput, Ident, LitInt, LitStr, Token, Type,
    parse::{Parse, ParseStream, Parser},
    parse_macro_input,
};

const ALPHABET: [&str; 10] = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];

#[proc_macro_derive(Serializable, attributes(enum_info, bitfields))]
pub fn derive_serializable(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let mut read_from = TokenStream::new();
    let mut write_to = TokenStream::new();

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

                            // panic!("{:?}", matches!(field.ty, Type::Path(_)));
                            match &field.ty {
                                Type::Path(ty_path) => {
                                    if ty_path
                                        .path
                                        .segments
                                        .iter()
                                        .next()
                                        .unwrap()
                                        .ident
                                        .to_string()
                                        != "bool"
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
                    _ => panic!("unimplemented"),
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
                    syn::Fields::Unit => {}
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

            let EnumInfo { ty, start_idx } = enum_info_attr.parse_args().unwrap();

            let mut idx = start_idx as usize;

            let mut num_to_variant: Vec<TokenStream> = Vec::new();
            let mut variant_to_num: Vec<TokenStream> = Vec::new();

            for variant in e.variants {
                let name = &variant.ident;
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
                idx += 1;
            }

            read_from = quote! {
                Ok(match <#ty>::read_from(buf)?.into_len() {
                    #(#num_to_variant,)*
                    x @ _ => return Err(crate::ReadingError::Message(format!("invalid enum index: {}",x)))
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
            panic!("unimplemented")
        }
    };
    let name = input.ident;

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
    start_idx: i32,
}

impl Parse for EnumInfo {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<EnumInfo> {
        let ty = input.parse()?;
        input.parse::<Token![,]>()?;
        let start_idx = input.parse::<LitInt>()?.base10_parse()?;
        Ok(EnumInfo { ty, start_idx })
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
    serde_json::from_str(include_str!("../../resources/packets.json"))
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
    // panic!(
    //     "{}{}",
    //     "minecraft:".to_owned() + &packet_name,
    //     PACKET_REGISTRY[state][direction]["minecraft:".to_owned() + &packet_name]
    // );
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

// #[proc_macro]
// pub fn generate_blocks(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
//     generate_registry("minecraft:block", "Block")
// }

// #[proc_macro]
// pub fn generate_items(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
//     generate_registry("minecraft:item", "Item")
// }

// #[proc_macro]
// pub fn generate_entities(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
//     generate_registry("minecraft:entity_type", "Entity")
// }

// #[proc_macro]
// pub fn generate_block_entities(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
//     generate_registry("minecraft:block_entity_type", "BlockEntity")
// }

// fn generate_registry(registry_name: &str, enum_name: &str) -> proc_macro::TokenStream {
//     let mut blocks: Vec<TokenStream> = Vec::new();
//     let mut ids: Vec<TokenStream> = Vec::new();
//     let mut id_to_block: Vec<TokenStream> = Vec::new();

//     for (key, val) in REGISTRIES[registry_name]["entries"]
//         .as_object()
//         .unwrap()
//         .iter()
//     {
//         let block_name = format_ident!("{}", key[10..].to_upper_camel_case());
//         let id: i32 = val["protocol_id"].as_i64().unwrap() as i32;

//         blocks.push(quote!( #block_name ));
//         ids.push(quote!( Self::#block_name => #id));
//         id_to_block.push(quote!( #id => Self::#block_name ));
//     }

//     let enum_ident = format_ident!("{}", enum_name);

//     quote! {
//         #[derive(Debug)]
//         pub enum #enum_ident {
//             #(#blocks),*
//         }

//         impl Registry for #enum_ident {
//             const REGISTRY_NAME: &str = #registry_name;

//             fn internal_id(&self) -> i32 {
//                 match self {
//                     #(#ids),*
//                 }
//             }

//             fn from_id(id: i32) -> Option<Self> {
//                 Some(match id {
//                     #(#id_to_block,)*
//                     _ => return None
//                 })
//             }
//         }
//     }
//     .into()
// }

// #[proc_macro]
// pub fn generate_blocks(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
//     let mut blocks: Vec<TokenStream> = Vec::new();
//     // let mut state_type_declarations: Vec<TokenStream> = Vec::new();

//     let mut structs: Vec<TokenStream> = Vec::new();
//     // let mut state_enums: Vec<TokenStream> = Vec::new();
//     let mut trait_impl: Vec<TokenStream> = Vec::new();

//     let mut struct_names: Vec<Ident> = Vec::new();

//     for (key, val) in BLOCK_STATE_REGISTRY.as_object().unwrap().iter() {
//         let block_name = format_ident!("{}", key[10..].to_upper_camel_case());
//         let struct_name = format_ident!(
//             "{}",
//             (key[10..].to_owned() + "_state").to_upper_camel_case()
//         );

//         struct_names.push(struct_name.clone());
//         blocks.push(quote!( #block_name(D::#struct_name) ));

//         if let Some(properties) = val["properties"].as_object() {
//             let mut struct_fields: Vec<TokenStream> = Vec::new();
//             for (property, v) in properties.iter() {
//                 let possible_values = v.to_string();

//                 let mut property_data_structure = match (
//                     property.as_str(),
//                     possible_values.as_str(),
//                 ) {
//                     ("facing", r#"["north","south","west","east"]"#) => "Facing4",
//                     ("face", r#"["floor","wall","ceiling"]"#) => "Face",
//                     ("half", r#"["upper","lower"]"#) => "Half",
//                     ("half", r#"["top","bottom"]"#) => "Half",
//                     ("hinge", r#"["left","right"]"#) => "Hinge",

//                     ("axis", r#"["x","y","z"]"#) => "Axis",
//                     ("type", r#"["top","bottom","double"]"#) => "SlabType",
//                     (
//                         "shape",
//                         r#"["straight","inner_left","inner_right","outer_left","outer_right"]"#,
//                     ) => "StairsShape",
//                     (
//                         "shape",
//                         r#"["north_south","east_west","ascending_east","ascending_west","ascending_north","ascending_south"]"#,
//                     ) => "RailShapeAlt",
//                     ("facing", r#"["north","east","south","west","up","down"]"#) => "Facing6",
//                     (_, r#"["none","low","tall"]"#) => "WallHeight",
//                     ("leaves", r#"["none","small","large"]"#) => "Leaves",
//                     ("attachment", r#"["floor","ceiling","single_wall","double_wall"]"#) => {
//                         "Attachment"
//                     }
//                     ("tilt", r#"["none","unstable","partial","full"]"#) => "Tilt",
//                     ("part", r#"["head","foot"]"#) => "BedPart",
//                     ("sculk_sensor_phase", r#"["inactive","active","cooldown"]"#) => {
//                         "SculkSensorPhase"
//                     }
//                     ("type", r#"["single","left","right"]"#) => "ChestType",
//                     ("mode", r#"["compare","subtract"]"#) => "RedstoneComparatorMode",
//                     ("orientation", r#"["down_east","down_north","down_south","down_west","up_east","up_north","up_south","up_west","west_up","east_up","north_up","south_up"]"#) => "Orientation",
//                     ("creaking_heart_state", r#"["uprooted","dormant","awake"]"#) => "CreakingHeartBlockState",
//                     ("facing", r#"["down","north","south","west","east"]"#) => "Facing5",
//                     ("type", r#"["normal","sticky"]"#) => "PistonType",
//                     ("axis", r#"["x","z"]"#) => "Axis2d",
//                     ("instrument", r#"["harp","basedrum","snare","hat","bass","flute","bell","guitar","chime","xylophone","iron_xylophone","cow_bell","didgeridoo","bit","banjo","pling","zombie","skeleton","creeper","dragon","wither_skeleton","piglin","custom_head"]"#) => "NoteBlockInstrument",
//                     ("thickness", r#"["tip_merge","tip","frustum","middle","base"]"#) => "DripstoneThickness",
//                     ("vertical_direction", r#"["up","down"]"#) => "VerticalDirection",
//                     ("shape", r#"["north_south","east_west","ascending_east","ascending_west","ascending_north","ascending_south","south_east","south_west","north_west","north_east"]"#) => "RailShape",
//                     (_, r#"["up","side","none"]"#) => "RedstoneOrientation",
//                     ("flower_amount", _) => "int",
//                     ("age", _) => "int",
//                     ("moisture",_) => "int",
//                     ("note", _) => "int",
//                     ("level", _) => "int",
//                     ("segment_amount", _) => "int",
//                     ("delay", _) => "int",
//                     ("charges", _) => "int",
//                     ("distance", _) => "int",
//                     ("pickles", _) => "int",
//                     ("hatch", _) => "int",
//                     ("layers", _) => "int",
//                     ("dusted", _) => "int",
//                     ("eggs", _) => "int",
//                     ("stage", _) => "int",
//                     ("candles", _) => "int",
//                     ("bites", _) => "int",
//                     ("honey_level", _) => "int",
//                     ("rotation", _) => "int",
//                     ("power", _) => "int",
//                     ("mode", r#"["save","load","corner","data"]"#) => "StructureBlockMode",
//                     ("mode", r#"["start","log","fail","accept"]"#) => "TestBlockMode",
//                     ("trial_spawner_state", r#"["inactive","waiting_for_players","active","waiting_for_reward_ejection","ejecting_reward","cooldown"]"#) => "TrialSpawnerBlockState",
//                     ("vault_state", r#"["inactive","active","unlocking","ejecting"]"#) => "VaultBlockState",
//                     (_, r#"["true","false"]"#) => "bool",
//                     _ => panic!("unknown property: {} and {}", property, possible_values),
//                 }.to_owned();

//                 let property_values = v.as_array().unwrap();

//                 let ty: TypePath = if property_data_structure == "int" {
//                     let ident = format_ident!("BoundedInt");
//                     let low = property_values[0].as_str().unwrap().parse::<u8>().unwrap();
//                     let high = property_values[property_values.len() - 1]
//                         .as_str()
//                         .unwrap()
//                         .parse::<u8>()
//                         .unwrap();

//                     parse_quote! { #ident<#low,#high> }
//                 } else {
//                     let ident = format_ident!("{}", property_data_structure);

//                     parse_quote! { #ident }
//                 };

//                 let field_name = if property == "type" { "ty" } else { property };
//                 let field_ident = format_ident!("{}", field_name);

//                 struct_fields.push(quote!( pub #field_ident: #ty ));
//             }
//             structs.push(quote! {
//                 pub struct #struct_name {
//                     #(#struct_fields),*
//                 }
//             });

//             // let d = syn::token::Lt::default();
//             trait_impl.push(quote!(type #struct_name = #struct_name;));
//         } else {
//             trait_impl.push(quote!(type #struct_name = ();));
//         };

//         let states = val["states"].as_array().unwrap();
//     }

//     let mut block_to_id: Vec<TokenStream> = Vec::new();
//     let mut id_to_block: Vec<TokenStream> = Vec::new();

//     for (key, val) in REGISTRIES["minecraft:block"]["entries"]
//         .as_object()
//         .unwrap()
//         .iter()
//     {
//         let block_name = format_ident!("{}", key[10..].to_upper_camel_case());
//         let id: i32 = val["protocol_id"].as_i64().unwrap() as i32;

//         block_to_id.push(quote!( Self::#block_name(_) => #id));
//         id_to_block.push(quote!( #id => Self::#block_name(()) ));
//     }

//     quote! {
//         pub struct Empty;
//         pub struct WithState;

//         #[derive(Debug)]
//         pub enum Block<D: BlockStateData> {
//             #(#blocks),*
//         }

//         pub trait BlockStateData {
//             #(type #struct_names;)*
//         }

//         impl BlockStateData for WithState{
//             #(#trait_impl)*
//         }

//         impl BlockStateData for Empty {
//             #(type #struct_names = ();)*
//         }

//         #(#structs)*

//         impl Registry for Block<Empty> {
//             const REGISTRY_NAME: &str = "minecraft:block";

//             fn internal_id(&self) -> i32 {
//                 match self {
//                     #(#block_to_id),*
//                 }
//             }

//             fn from_id(id: i32) -> Option<Self> {
//                 Some(match id {
//                     #(#id_to_block,)*
//                     _ => return None
//                 })
//             }
//         }
//     }
//     .into()
// }
