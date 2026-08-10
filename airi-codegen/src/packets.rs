use indexmap::IndexMap;

use heck::ToShoutySnekCase;
use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;

pub fn build() -> TokenStream {
    let packet_data: IndexMap<String, IndexMap<String, IndexMap<String, i32>>> =
        serde_json::from_slice(include_bytes!("../assets/packets.json")).unwrap();

    // packet -> (dir,state,id)
    let mut seen_packets: IndexMap<String, (String, String, i32)> = IndexMap::new();

    for (state, dirs) in packet_data {
        for (dir, packets) in dirs {
            for (mut packet, id) in packets {
                packet = packet.strip_prefix("minecraft:").unwrap().to_owned();
                insert_recurse(&mut seen_packets, packet, &dir, &state, id);
            }
        }
    }

    let mut packet_declarations = Vec::new();

    for (packet, (_, _, id)) in seen_packets {
        let var_name = format_ident!("{}", packet.replace('/', "_").TO_SHOUTY_SNEK_CASE());
        packet_declarations.push(quote! { pub const #var_name: i32 = #id; });
    }

    quote! {
        #(#packet_declarations)*
    }
}

// this was so much more work than necessary omfg
fn insert_recurse(
    seen_packets: &mut IndexMap<String, (String, String, i32)>,
    mut packet: String,
    dir: &String,
    state: &String,
    id: i32,
) {
    // shift_remove is O(n) but maintains order
    if let Some(other) = seen_packets.shift_remove(&packet) {
        if &other.0 == dir {
            packet.push('_');
            insert_recurse(
                seen_packets,
                packet.clone() + &other.1,
                &other.0,
                &other.1,
                other.2,
            );
            insert_recurse(seen_packets, packet.clone() + state, dir, state, id);
        } else {
            insert_recurse(
                seen_packets,
                other.0.clone() + "_" + &packet,
                &other.0,
                &other.1,
                other.2,
            );
            insert_recurse(seen_packets, dir.clone() + "_" + &packet, dir, state, id);
        }
    } else {
        seen_packets.insert(packet.clone(), (dir.clone(), state.clone(), id));
    }
}
