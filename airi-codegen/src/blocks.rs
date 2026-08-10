use std::{collections::HashMap, fs::File, iter};

use heck::ToPascalCase;
use indexmap::IndexMap;
use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn build() -> TokenStream {
    // panic!("{:?}", std::env::current_dir());
    let file = File::open("./airi-codegen/assets/block_properties.json").unwrap();
    let block_properties: IndexMap<String, IndexMap<String, Vec<String>>> =
        serde_json::from_reader(file).unwrap();

    let mut block_variants = Vec::new();
    let mut property_structs = Vec::new();
    let mut state_enums = Vec::new();
    let mut struct_impls = Vec::new();

    let _block_state_id = 0;

    // property base name -> Vec<(states, assigned_enum_name)>
    let mut seen_properties: HashMap<String, Vec<(Vec<String>, String)>> = HashMap::new();

    for (block, properties) in &block_properties {
        let block_id = block.strip_prefix("block.minecraft").unwrap();

        let variant_name = format_ident!("{}", block_id.to_pascal_case());
        let struct_name = &variant_name;

        block_variants.push(quote! { #variant_name(#struct_name) });

        let mut struct_fields = Vec::new();

        for (property, states) in properties {
            let entries = seen_properties.entry(property.clone()).or_default();
            let assigned_name = if let Some((_, existing_name)) = entries
                .iter()
                .find(|(existing_states, _)| existing_states == states)
            {
                // same name + same states seen before -> reuse that enum
                existing_name.clone()
            } else {
                // same name but different states -> mint a new numbered variant
                let n = entries.len();
                let candidate = if n == 0 {
                    property.clone()
                } else {
                    format!("{}{}", property, variant_name)
                };
                entries.push((states.clone(), candidate.clone()));

                let state_enum_name = format_ident!("{}", candidate.to_pascal_case());
                let state_variants = states.iter().map(|s| {
                    let prefix = if s.chars().next().unwrap().is_ascii_digit() {
                        "Num"
                    } else {
                        ""
                    };
                    format_ident!("{}{}", prefix, s.to_pascal_case())
                });
                state_enums.push(quote! {enum #state_enum_name {#(#state_variants),*}});

                candidate
            };

            let state_enum_name = format_ident!("{}", assigned_name.to_pascal_case());

            let property2 = if property == "type" { "ty" } else { property };
            let property_ident = format_ident!("{}", property2);
            struct_fields.push(quote! { #property_ident: #state_enum_name });
        }

        property_structs.push(quote! { struct #struct_name { #(#struct_fields),*} });

        // used for block ids
        for _product in properties
            .iter()
            .map(|(property, states)| iter::repeat(property).zip(states))
            .multi_cartesian_product()
        {
            let struct_impl = quote! {

                // impl
            };

            struct_impls.push(struct_impl)
        }
    }

    quote! {
        pub enum Block {
            #(#block_variants),*
        }

        #(#property_structs)*

        #(#state_enums)*

        #(#struct_impls)*
    }
}

pub trait BlockType {
    fn block_state_id(&self) -> u16;
}
