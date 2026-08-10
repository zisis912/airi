use std::fmt::Debug;

use crate::bitset::BitSet;
use crate::bitset::FixedBitSet;
use crate::slot::ColorI32;
use crate::slot::Component;
use crate::slot::HashedSlot;
use crate::slot::PaintingVariant;
use crate::slot::Slot;

use super::*;
use airi_macros::Serializable;

pub const PROTOCOL_VERSION: i32 = 776;

macro_rules! state_packets {
    (
        $($dirName:ident $dir:ident {
            $($stateName:ident $state:ident {
                $($(#[$attr:meta])*$packet:ident $resource_id:ident {
                    $($(#[$fattr:meta])*$field:ident $ty:ty)*
                })*
            })+
        })+
    ) => {
        $(
            pub mod $dir {
            $(
                pub mod $state {
                #![allow(unused_imports)]

                use crate::*;
                use packet::*;
                use slot::*;
                use airi_data::packets::*;
                $(


                    #[derive(Serializable, Debug)]
                    $(#[$attr])*
                    pub struct $packet {
                        $($(#[$fattr])* pub $field:$ty,)*
                    }

                    impl PacketType for $packet {
                        const ID: i32 = airi_data::packets::$resource_id;
                    }
                )*
                }
            )+
            }
        )+

        #[derive(Debug)]
        pub enum Packet {
            $($($($packet($dir::$state::$packet),)*)+)+
        }

        use std::io::Write;

        impl Packet {
            pub fn write<W: Write>(&self, buf: &mut W) -> Result<(),WritingError> {
                match self {
                    $($($(Self::$packet(p) => p.write(buf)?,)*)+)+
                }
                Ok(())
            }
        }

        /// Reads a packet from a buffer, given its id, `State` and `Direction`. The data in `buf`
        /// must be in clear form (no encryption/compression). See the main page for usage examples.
        pub fn packet_by_id<R: io::Read>(state: State, dir: Direction, id: i32, buf: &mut R) -> Result<Packet, ReadingError> {
            // println!("{:?} {:?} {:?}",state,dir,id);
            Ok(match dir {
                $(
                    Direction::$dirName => match state {
                        $( State::$stateName => match id {
                            $($dir::$state::$packet::ID => Packet::$packet($dir::$state::$packet::read_from(buf)?),)*
                            _=>return Err(ReadingError::Message(format!("invalid packet id: {:#04x}",id)))
                            }
                        )+
                        #[allow(unreachable_patterns)]
                        _ => return Err(ReadingError::Message(format!("invalid packet state: {:?}",state)))
                    }
                )+
            })
        }
    };
}

#[derive(Clone, Copy, Debug)]
pub enum State {
    Handshake,
    Status,
    Login,
    Configuration,
    Play,
}

#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Serverbound,
    Clientbound,
}

pub trait PacketType: Serializable + Debug {
    const ID: i32;

    /// Write the packet's ID as a VarInt, then the packet itself into `buf`.
    ///
    /// [`PacketType::write()`] and [`Packet::write()`] should be **heavily** preferred
    /// over using `Serializable::write_to()` on the underlying packet data types,
    /// since there will never be a scenario where you need to write a packet without
    /// preceding it with its ID.
    fn write<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        VarInt(Self::ID).write_to(buf)?;
        self.write_to(buf)?;
        Ok(())
    }
}

// TODO: change string id for hex literal ID
state_packets! {
    Serverbound c2s {
        Handshake handshake {
            Handshake INTENTION {
                protocol_version VarInt
                server_address String
                server_port u16
                intent Intent
            }
        }
        Status status {
            StatusRequest STATUS_REQUEST {}
            PingRequestStatus PING_REQUEST_STATUS {
                timestamp i64
            }
        }
        Login login {
            LoginStart SERVERBOUND_HELLO {
                name String
                player_uuid UUID
            }
            EncryptionResponse KEY {
                shared_secret LenPrefixedBytes<VarInt>
                verify_token LenPrefixedBytes<VarInt>
            }
            LoginPluginResponse CUSTOM_QUERY_ANSWER {
                message_id VarInt
                data UnsizedBytes
            }
            LoginAcknowledged LOGIN_ACKNOWLEDGED {}
            CookieResponseLogin COOKIE_RESPONSE {
                key Identifier
                payload Option<LenPrefixedBytes<VarInt>>
            }
        }
        Configuration configuration {
            ClientInformationConfiguration CLIENT_INFORMATION_CONFIGURATION {
                locale String
                view_distance i8
                chat_mode ChatMode
                chat_colors bool
                displayed_skin_parts SkinParts
                main_hand MainHand
                enable_text_filtering bool
                allow_server_listings bool
                particle_status ParticleStatus
            }
            CookieResponseConfiguration COOKIE_RESPONSE_CONFIGURATION {
                key Identifier
                payload Option<LenPrefixedBytes<VarInt>>
            }
            ServerboundPluginMessageConfiguration SERVERBOUND_CUSTOM_PAYLOAD_CONFIGURATION {
                channel Identifier
                data UnsizedBytes
            }
            AcknowledgeFinishConfiguration SERVERBOUND_FINISH_CONFIGURATION {}
            ServerboundKeepAliveConfiguration SERVERBOUND_KEEP_ALIVE_CONFIGURATION {
                keep_alive_id i64
            }
            PongConfiguration PONG_CONFIGURATION {
                id i32
            }
            ResourcePackResponseConfiguration RESOURCE_PACK_CONFIGURATION {
                uuid UUID
                result ResourcePackResult
            }
            ServerboundKnownPacks SERVERBOUND_SELECT_KNOWN_PACKS {
                known_packs PrefixedArray<KnownPack>
            }
            CustomClickActionConfiguration ACCEPT_CODE_OF_CONDUCT {}
        }
        Play play {
            ConfirmTeleportation ACCEPT_TELEPORTATION {
                teleport_id VarInt
            }
            QueryBlockEntityTag BLOCK_ENTITY_TAG_QUERY {
                transaction_id VarInt
                location Position
            }
            BundleItemSelected BUNDLE_ITEM_SELECTED {
                slot_of_bundle VarInt
                slot_in_bundle VarInt
            }
            ChangeDifficultyServerbound SERVERBOUND_CHANGE_DIFFICULTY {
                new_difficulty Difficulty
            }
            AcknowledgeMessage CHAT_ACK {
                message_count VarInt
            }
            ChatCommand CHAT_COMMAND {
                command String
            }
            SignedChatCommand CHAT_COMMAND_SIGNED {
                command String
                timestamp i64
                salt u64
                argument_signatures PrefixedArray<ArgumentSignature>
                message_count VarInt
                acknowledged FixedBitSet<20>
                checksum u8
            }
            ChatMessage CHAT {
                message String
                timestamp i64
                salt u64
                signature Option<StaticLenBytes<256>>
                message_count VarInt
                // offset VarInt
                acknowledged FixedBitSet<20>
                checksum u8
            }
            PlayerSession CHAT_SESSION_UPDATE {
                session_id UUID
                // public key info
                expires_at i64
                public_key LenPrefixedBytes<VarInt>
                key_signature LenPrefixedBytes<VarInt>
            }
            ChunkBatchReceived CHUNK_BATCH_RECEIVED {
                chunks_per_tick f32
            }
            ClientStatus CLIENT_COMMAND {
                action_id VarInt
            }
            ClientTickEnd CLIENT_TICK_END {}
            ClientInformationPlay CLIENT_INFORMATION_PLAY {
                locale String
                view_distance i8
                chat_mode ChatMode
                chat_colors bool
                displayed_skin_parts SkinParts
                main_hand MainHand
                enable_text_filtering bool
                allow_server_listings bool
                particle_status ParticleStatus
            }
            CommandSuggestionsRequest COMMAND_SUGGESTION {
                transaction_id VarInt
                text String
            }
            AcknowledgeConfiguration CONFIGURATION_ACKNOWLEDGED {}
            ClickContainerButton CONTAINER_BUTTON_CLICK {
                window_id VarInt
                button_id VarInt
            }
            ClickContainer CONTAINER_CLICK {
                window_id VarInt
                state_id VarInt
                slot i16
                button i8
                mode VarInt
                changed_slots PrefixedArray<ChangedSlot>
                carried_item HashedSlot
            }
            CloseContainerServerbound SERVERBOUND_CONTAINER_CLOSE {
                window_id VarInt
            }
            ChangeContainerSlotState CONTAINER_SLOT_STATE_CHANGED {
                slot_id VarInt
                window_id VarInt
                state bool
            }
            CookieResponsePlay COOKIE_RESPONSE_PLAY {
                key Identifier
                payload Option<LenPrefixedBytes<VarInt>>
            }
            ServerboundPluginMessagePlay SERVERBOUND_CUSTOM_PAYLOAD_PLAY {
                channel Identifier
                data UnsizedBytes
            }
            DebugSampleSubscription DEBUG_SUBSCRIPTION_REQUEST {
                subscriptions PrefixedArray<VarInt>
            }
            EditBook EDIT_BOOK {
                slot VarInt
                entries PrefixedArray<String>
                title Option<String>
            }
            QueryEntityTag ENTITY_TAG_QUERY {
                transaction_id VarInt
                entity_id VarInt
            }
            Interact INTERACT {
                entity_id VarInt
                interaction InteractionType
                sneak_key_pressed bool
            }
            JigsawGenerate JIGSAW_GENERATE {
                location Position
                levels VarInt
                keep_jigsaws bool
            }
            ServerboundKeepAlivePlay SERVERBOUND_KEEP_ALIVE_PLAY {
                keep_alive_id i64
            }
            LockDifficulty LOCK_DIFFICULTY {
                locked bool
            }
            SetPlayerPosition MOVE_PLAYER_POS {
                x f64
                feet_y f64
                z f64
                flags MovePlayerFlags
            }
            SetPlayerPositionAndRotation MOVE_PLAYER_POS_ROT {
                x f64
                feet_y f64
                z f64
                yaw f32
                pitch f32
                flags MovePlayerFlags
            }
            SetPlayerRotation MOVE_PLAYER_ROT {
                yaw f32
                pitch f32
                flags MovePlayerFlags
            }
            SetPlayerMovementFlags MOVE_PLAYER_STATUS_ONLY {
                flags MovePlayerFlags
            }
            MoveVehicleServerbound SERVERBOUND_MOVE_VEHICLE {
                pos Vec3<f64>
                yaw f32
                pitch f32
                on_ground bool
            }
            PaddleBoat PADDLE_BOAT {
                left_padle_turning bool
                right_padle_turning bool
            }
            PickItemFromBlock PICK_ITEM_FROM_BLOCK {
                location Position
                include_data bool
            }
            PickItemFromEntity PICK_ITEM_FROM_ENTITY {
                entity_id VarInt
                include_data bool
            }
            PingRequestPlay PING_REQUEST_PLAY {
                payload i64
            }
            PlaceReciple PLACE_RECIPE {
                window_id VarInt
                recipe_id VarInt
                make_all bool
            }
            PlayerAbilitiesServerbound SERVERBOUND_PLAYER_ABILITIES {
                flags u8 // TODO: bitmask struct
            }
            PlayerAction PLAYER_ACTION {
                status VarInt
                location Position
                face i8
                sequence VarInt
            }
            PlayerCommand PLAYER_COMMAND {
                entity_id VarInt
                action_id VarInt // TODO: ENUM
                jump_boost VarInt
            }
            PlayerInputServerbound PLAYER_INPUT {
                flags PlayerInput
            }
            PlayerLoaded PLAYER_LOADED {}
            PongPlay PONG_PLAY {
                id i32
            }
            ChangeRecipeBookSettings RECIPE_BOOK_CHANGE_SETTINGS {
                book_id VarInt
                book_open bool
                filter_active bool
            }
            SetSeenRecipe RECIPE_BOOK_SEEN_RECIPE {
                recipe_id VarInt
            }
            RenameItem RENAME_ITEM {
                item_name String
            }
            ResourcePackResponsePlay RESOURCE_PACK_PLAY {
                uuid UUID
                result ResourcePackResult
            }
            SeenAdvancements SEEN_ADVANCEMENTS {
                action SeenAdvancementsAction
            }
            SelectTrade SELECT_TRADE {
                selected_slot VarInt
            }
            SetBeaconEffect SET_BEACON {
                primary_effect Option<VarInt>
                secondary_effect Option<VarInt>
            }
            SetHeldItemServerbound SET_CARRIED_ITEM {
                slot i16
            }
            ProgramCommandBlock SET_COMMAND_BLOCK {
                location Position
                command String
                mode VarInt
                flags u8
            }
            ProgramCommandBlockMinecart SET_COMMAND_MINECART {
                entity_id VarInt
                command String
                track_output bool
            }
            SetCreativeModeSlot SET_CREATIVE_MODE_SLOT {
                slot i16
                clicked_item Slot
            }
            ProgramJigsawBlock SET_JIGSAW_BLOCK {
                location Position
                name Identifier
                target Identifier
                pool Identifier
                final_state String
                joint_type String
                selection_priority VarInt
                placement_priority VarInt
            }
            ProgramStructureBlock SET_STRUCTURE_BLOCK {
                location Position
                action VarInt
                mode VarInt
                name String
                offset Vec3<i8>
                size Vec3<i8>
                mirror VarInt
                rotation VarInt
                metadata String
                integrity f32
                seed VarLong
                flags u8
            }
            SetTestBlock SET_TEST_BLOCK {
                position Position
                mode VarInt
                message String
            }
            UpdateSign SIGN_UPDATE {
                location Position
                is_front_text bool
                line1 String
                line2 String
                line3 String
                line4 String
            }
            SwingArm SWING {
                hand Hand
            }
            TeleportToEntity TELEPORT_TO_ENTITY {
                target_player UUID
            }
            TestInstanceBlockAction TEST_INSTANCE_BLOCK_ACTION {
                position Position
                action VarInt
                test Option<Identifier>
                size Vec3<VarInt>
                rotation VarInt
                ignore_entities bool
                status VarInt
                error_message Option<TextComponent>
            }
            UseItemOn USE_ITEM_ON {
                hand Hand
                location Position
                face VarInt
                cursor_position Vec3<f32>
                inside_block bool
                world_border_hit bool
                sequence VarInt
            }
            UseItem USE_ITEM {
                hand Hand
                sequence VarInt
                yaw f32
                pitch f32
            }
            CustomClickActionPlay CUSTOM_CLICK_ACTION_PLAY {
                id Identifier
                payload nbt::Tag
            }
        }
    }
    Clientbound s2c {
        Status status {
            StatusResponse STATUS_RESPONSE {
                json_response String
            }
            PongResponseStatus PONG_RESPONSE_STATUS {
                timestamp i64
            }
        }
        Login login {
            LoginDisconnect LOGIN_DISCONNECT {
                reason JsonTextComponent
            }
            EncryptionRequest CLIENTBOUND_HELLO {
                /// Always empty when sent by the vanilla server.
                server_id String
                public_key LenPrefixedBytes<VarInt>
                verify_token LenPrefixedBytes<VarInt>
                should_authenticate bool
            }
            LoginSuccess LOGIN_FINISHED {
                profile GameProfile
                session_id UUID
            }
            SetCompression LOGIN_COMPRESSION {
                theshold VarInt
            }
            LoginPluginRequest CUSTOM_QUERY {
                message_id VarInt
                channel Identifier
                data UnsizedBytes
            }
            CookieRequestLogin COOKIE_REQUEST {
                key Identifier
            }
        }
        Configuration configuration {
            CookieRequestConfiguration COOKIE_REQUEST_CONFIGURATION {
                key Identifier
            }
            ClientboundPluginMessageConfiguration CLIENTBOUND_CUSTOM_PAYLOAD_CONFIGURATION {
                channel Identifier
                data UnsizedBytes
            }
            DisconnectConfiguration DISCONNECT_CONFIGURATION {
                reason TextComponent
            }
            FinishConfiguration CLIENTBOUND_FINISH_CONFIGURATION {}
            ClientboundKeepAliveConfiguration CLIENTBOUND_KEEP_ALIVE_CONFIGURATION {
                keep_alive_id i64
            }
            PingConfiguration PING_CONFIGURATION {
                id i32
            }
            ResetChat RESET_CHAT {}
            RegistryData REGISTRY_DATA {
                registry_id Identifier
                entries PrefixedArray<RegistryEntry>
            }
            RemoveResourcePackConfiguration RESOURCE_PACK_POP_CONFIGURATION {
                uuid Option<UUID>
            }
            AddResourcePackConfiguration RESOURCE_PACK_PUSH_CONFIGURATION {
                uuid UUID
                url String
                hash String
                forced bool
                prompt_message Option<TextComponent>
            }
            StoreCookieConfiguration STORE_COOKIE_CONFIGURATION {
                key Identifier
                payload LenPrefixedBytes<VarInt>
            }
            TransferConfiguration TRANSFER_CONFIGURATION {
                host String
                port VarInt
            }
            FeatureFlags UPDATE_ENABLED_FEATURES {
                feature_flags PrefixedArray<Identifier>
            }
            UpdateTagsConfiguration UPDATE_TAGS_CONFIGURATION {
                tags PrefixedArray<Tags>
            }
            ClientboundKnownPacks CLIENTBOUND_SELECT_KNOWN_PACKS {
                known_packs PrefixedArray<KnownPack>
            }
            CustomReportDetailsConfiguration CUSTOM_REPORT_DETAILS_CONFIGURATION {
                details PrefixedArray<ReportDetail>
            }
            ServerLinksConfiguration SERVER_LINKS_CONFIGURATION {
                links PrefixedArray<ServerLink>
            }
            ClearDialogConfiguration CLEAR_DIALOG_CONFIGURATION {}
            ShowDialogConfiguration SHOW_DIALOG_CONFIGURATION {
                dialog nbt::Tag
            }
            CodeOfConduct CODE_OF_CONDUCT {
                code_of_conduct String
            }
        }
        Play play {
            BundleDelimiter BUNDLE_DELIMITER {}
            SpawnEntity ADD_ENTITY {
                entity_id VarInt
                entity_uuid UUID
                ty VarInt
                position Vec3<f64>
                velocity LpVec3
                pitch Angle
                yaw Angle
                head_yaw Angle
                data VarInt
            }
            EntityAnimation ANIMATE {
                entity_id VarInt
                animation Animation
            }
            AwardStatistics AWARD_STATS {
                statistics PrefixedArray<StatisticEntry>
            }
            AcknowledgeBlockChange BLOCK_CHANGED_ACK {
                sequence_id VarInt
            }
            SetBlockDestroyStage BLOCK_DESTRUCTION {
                entity_id VarInt
                location Position
                destroy_stage u8
            }
            BlockEntityData BLOCK_ENTITY_DATA {
                location Position
                ty VarInt
                nbt nbt::Tag
            }
            BlockAction BLOCK_EVENT {
                location Position
                action_id u8
                action_parameter u8
                block_type VarInt
            }
            BlockUpdate BLOCK_UPDATE {
                location Position
                block_state_id VarInt
            }
            BossBar BOSS_EVENT {
                uuid UUID
                action BossAction
            }
            ChangeDifficultyClientbound CLIENTBOUND_CHANGE_DIFFICULTY {
                difficulty Difficulty
                difficulty_locked bool
            }
            ChunkBatchFinished CHUNK_BATCH_FINISHED {
                batch_size VarInt
            }
            ChunkBatchStart CHUNK_BATCH_START {}
            ChunkBiomes CHUNKS_BIOMES {
                chunk_biome_data PrefixedArray<ChunkBiomeData>
            }
            ClearTitles CLEAR_TITLES {
                reset bool
            }
            CommandSuggestionsResponse COMMAND_SUGGESTIONS {
                id VarInt
                start VarInt
                length VarInt
                matches PrefixedArray<CommandSuggestionMatch>
            }
            Commands COMMANDS {
                nodes PrefixedArray<Node>
                root_index VarInt
            }
            CloseContainerClientbound CLIENTBOUND_CONTAINER_CLOSE {
                window_id VarInt
            }
            SetContainerContent CONTAINER_SET_CONTENT {
                window_id VarInt
                state_id VarInt
                slot_data PrefixedArray<Slot>
                carried_item Slot
            }
            SetContainerProperty CONTAINER_SET_DATA {
                window_id VarInt
                property i16
                value i16
            }
            SetContainerSlot CONTAINER_SET_SLOT {
                window_id VarInt
                state_id VarInt
                slot i16
                slot_data Slot
            }
            CookieRequest COOKIE_REQUEST_PLAY {
                key Identifier
            }
            SetCooldown COOLDOWN {
                cooldown_group Identifier
                cooldown_ticks VarInt
            }
            ChatSuggestions CUSTOM_CHAT_COMPLETIONS {
                action ChatSuggestionAction
                entries PrefixedArray<String>
            }
            ClientboundPluginMessagePlay CLIENTBOUND_CUSTOM_PAYLOAD_PLAY {
                channel Identifier
                data UnsizedBytes
            }
            DamageEvent DAMAGE_EVENT {
                entity_id VarInt
                source_type_id VarInt
                source_cause_id VarInt
                source_direct_id VarInt
                source_position Option<Vec3<f64>>
            }
            DebugBlockValue DEBUG_BLOCK_VALUE {
                location Position
                update DebugSubscriptionUpdate
            }
            DebugChunkValue DEBUG_CHUNK_VALUE {
                chunk_x i32
                chunk_z i32
                update DebugSubscriptionUpdate
            }
            DebugEntityValue DEBUG_ENTITY_VALUE {
                entity_id VarInt
                upate DebugSubscriptionUpdate
            }
            DebugEvent DEBUG_EVENT {
                event DebugSubscriptionUpdate
            }
            DebugSample DEBUG_SAMPLE {
                sample PrefixedArray<i64>
                sample_type DebugSampleType
            }
            DeleteMessage DELETE_CHAT {
                message_id_or_signature IdOrX<UnsizedBytes>
            }
            DisconnectPlay DISCONNECT_PLAY {
                reason TextComponent
            }
            DisguisedChatMessage DISGUISED_CHAT {
                message TextComponent
                chat_type IdOrX<ChatType>
                sender_name TextComponent
                target_name Option<TextComponent>
            }
            EntityEvent ENTITY_EVENT {
                entity_id i32
                entity_status i8
            }
            TeleportEntity ENTITY_POSITION_SYNC {
                entity_id VarInt
                position Vec3<f64>
                velocity Vec3<f64>
                yaw f32
                pitch f32
                on_ground bool
            }
            Explosion EXPLODE {
                position Vec3<f64>
                player_delta_velocity Option<Vec3<f64>>
                explosion_particle Particle
                explosion_sound IdOrX<SoundEvent>
                block_particle_alternatives PrefixedArray<BlockParticleAlternative>
            }
            UnloadChunk FORGET_LEVEL_CHUNK {
                chunk_x i32
                chunk_z i32
            }
            GameEvent GAME_EVENT {
                event u8
                value f32
            }
            GameTestHighlightPosition GAME_TEST_HIGHLIGHT_POS {
                absolute_location Position
                relative_location Position
            }
            OpenHorseScreen MOUNT_SCREEN_OPEN {
                window_id VarInt
                inventory_columns_count VarInt
                entity_id i32
            }
            HurtAnimation HURT_ANIMATION {
                entity_id VarInt
                yaw f32
            }
            InitializeWorldBorder INITIALIZE_BORDER {
                x f64
                z f64
                old_diameter f64
                new_diameter f64
                speed VarLong
                portal_teleport_boundary VarInt
                warning_blocks VarInt
                warning_time VarInt
            }
            ClientboundKeepAlivePlay CLIENTBOUND_KEEP_ALIVE_PLAY {
                keep_alive_id i64
            }
            ChunkDataAndUpdateLight LEVEL_CHUNK_WITH_LIGHT {
                chunk_x i32
                chunk_z i32
                heightmaps PrefixedArray<HeightMap>
                data LenPrefixedBytes<VarInt>
                block_entities PrefixedArray<BlockEntity>
                light LightData
            }
            WorldEvent LEVEL_EVENT {
                event i32
                location Position
                data i32
                disable_relative_volume bool
            }
            Particles LEVEL_PARTICLES {
                long_distance bool
                always_visible bool
                position Vec3<f64>
                offset Vec3<f32>
                max_speed f32
                particle_count i32
                particle Particle
            }
            UpdateLight LIGHT_UPDATE {
                chunk_x VarInt
                chunk_z VarInt
                data LightData
            }
            LoginPlay LOGIN {
                entity_id i32
                is_hardcore bool
                dimension_names PrefixedArray< Identifier>
                max_players VarInt
                view_distance VarInt
                simulation_distance VarInt
                reduced_debug_info bool
                enable_respawn_screen bool
                do_limited_crafting bool
                dimension_type VarInt
                dimension_name Identifier
                hashed_seed i64
                game_mode u8
                previous_game_mode i8
                is_debug bool
                is_flat bool
                death_info Option<DeathInfo>
                portal_cooldown VarInt
                sea_level VarInt
                online_mode bool
                enforces_secure_chat bool
            }
            MapData MAP_ITEM_DATA {
                map_id VarInt
                scale i8
                locked bool
                icons Option<PrefixedArray<MapIcon>>
                color_patch MapColorPatch
            }
            MerchantOffers MERCHANT_OFFERS {
                window_id VarInt
                trades PrefixedArray<MerchantTrade>
                villager_level VarInt
                experience VarInt
                is_regular_villager bool
                can_restock bool
            }
            UpdateEntityPosition MOVE_ENTITY_POS {
                entity_id VarInt
                delta Vec3<i16>
                on_ground bool
            }
            UpdateEntityPositionAndRotation MOVE_ENTITY_POS_ROT {
                entity_id VarInt
                delta Vec3<i16>
                yaw Angle
                pitch Angle
                on_ground bool
            }
            MoveMinecraftAlongTrack MOVE_MINECART_ALONG_TRACK {
                entity_id VarInt
                steps PrefixedArray<MinecartStep>
            }
            UpdateEntityRotation MOVE_ENTITY_ROT {
                entity_id VarInt
                yaw Angle
                pitch Angle
                on_ground bool
            }
            MoveVehicleClientbound CLIENTBOUND_MOVE_VEHICLE {
                pos Vec3<f64>
                yaw f32
                pitch f32
            }
            OpenBook OPEN_BOOK {
                hand Hand
            }
            OpenScreen OPEN_SCREEN {
                window_id VarInt
                window_type VarInt
                window_title TextComponent
            }
            OpenSignEditor OPEN_SIGN_EDITOR {
                location Position
                is_front_text bool
            }
            PingPlay PING_PLAY {
                id i32
            }
            PingResponsePlay PONG_RESPONSE_PLAY {
                payload i64
            }
            PlaceGhostRecipe PLACE_GHOST_RECIPE {
                window_id VarInt
                recipe_display RecipeDisplay
            }
            PlayerAbilitiesClientbound CLIENTBOUND_PLAYER_ABILITIES {
                flags PlayerAbilitiesFlags
                flying_speed f32
                fov_modifier f32
            }
            PlayerChatMessage PLAYER_CHAT {
                global_index VarInt
                sender UUID
                index VarInt
                message_signature Option<StaticLenBytes<256>>
                message String
                timestamp i64 /// ms since epoch
                salt u64
                message_ids_or_signatures PrefixedArray<IdOrX<StaticLenBytes<256>>>
                unsigned_content Option<TextComponent>
                filter_type ChatMessageFilterType
                chat_type IdOrX<ChatType>
                sender_name TextComponent
                target_name Option<TextComponent>
            }
            EndCombat PLAYER_COMBAT_END {
                duration VarInt
            }
            EnterCombat PLAYER_COMBAT_ENTER {}
            CombatDeath PLAYER_COMBAT_KILL {
                player_id VarInt
                message TextComponent
            }
            PlayerInfoRemove PLAYER_INFO_REMOVE {
                uuids PrefixedArray<UUID>
            }
            PlayerInfoUpdate PLAYER_INFO_UPDATE {
                actions PlayersActionsData
            }
            LookAt PLAYER_LOOK_AT {
                feet_eyes FeetEyes
                target Vec3<f64>
                is_entity Option<LookAtEntityInfo>
            }
            SynchronizePlayerPosition PLAYER_POSITION {
                teleport_id VarInt
                pos Vec3<f64>
                velocity Vec3<f64>
                yaw f32
                pitch f32
                flags TeleportFlags
            }
            PlayerRotation PLAYER_ROTATION {
                yaw f32
                relative_yaw bool
                pitch f32
                relative_pitch bool
            }
            RecipeBookAdd RECIPE_BOOK_ADD {
                recipes PrefixedArray<Recipe>
                replace bool
            }
            RecipeBookRemove RECIPE_BOOK_REMOVE {
                recipes PrefixedArray<VarInt>
            }
            RecipeBookSettings RECIPE_BOOK_SETTINGS {
                crafting_recipe_book_open bool
                crafting_recipe_filter_active bool
                smelting_recipe_filter_active bool
                smelting_recipe_book_open bool
                blast_furnace_recipe_filter_active bool
                blast_furnace_recipe_book_open bool
                smoker_recipe_filter_active bool
                smoker_recipe_book_open bool
            }
            RemoveEntities REMOVE_ENTITIES {
                entity_ids PrefixedArray<VarInt>
            }
            RemoveEntityEffect REMOVE_MOB_EFFECT {
                entity_id VarInt
                effect_id VarInt
            }
            ResetScore RESET_SCORE {
                entity_name String
                objective_name Option<String>
            }
            RemoveResourcePackPlay RESOURCE_PACK_POP_PLAY {
                uuid Option<UUID>
            }
            AddResourcePackPlay RESOURCE_PACK_PUSH_PLAY {
                uuid UUID
                url String
                hash String
                forced bool
                prompt_message Option<TextComponent>
            }
            Respawn RESPAWN {
                dimension_type VarInt
                dimension_name Identifier
                hashed_seed i64
                game_mode u8
                previous_game_mode i8
                is_debug bool
                is_flat bool
                death_info Option<DeathInfo>
                portal_cooldown VarInt
                sea_level VarInt
                data_kept DataKept
            }
            SetHeadRotation ROTATE_HEAD {
                entity_id VarInt
                head_yaw Angle
            }
            UpdateSectionBlocks SECTION_BLOCKS_UPDATE {
                chunk_section_position i64
                blocks PrefixedArray<VarLong>
            }
            SelectAdvancementsTab SELECT_ADVANCEMENTS_TAB {
                identifier Option<Identifier>
            }
            ServerData SERVER_DATA {
                motd TextComponent
                icon Option<LenPrefixedBytes<VarInt>>
            }
            SetActionBarText SET_ACTION_BAR_TEXT {
                action_bar_text TextComponent
            }
            SetBorderCenter SET_BORDER_CENTER {
                x f64
                z f64
            }
            SetBorderLerpSize SET_BORDER_LERP_SIZE {
                old_diameter f64
                new_diamter f64
                speed VarLong
            }
            SetBorderSize SET_BORDER_SIZE {
                diameter f64
            }
            SetBorderWarningDelay SET_BORDER_WARNING_DELAY {
                warning_time VarInt
            }
            SetBorderWarningDistance SET_BORDER_WARNING_DISTANCE {
                warning_blocks VarInt
            }
            SetCamera SET_CAMERA {
                camera_id VarInt
            }
            SetCenterChunk SET_CHUNK_CACHE_CENTER {
                chunk_x VarInt
                chunk_z VarInt
            }
            SetRenderDistance SET_CHUNK_CACHE_RADIUS {
                view_distance VarInt
            }
            SetCursorItem SET_CURSOR_ITEM {
                carried_item Slot
            }
            SetDefaultSpawnPosition SET_DEFAULT_SPAWN_POSITION {
                dimension_name Identifier
                location Position
                yaw f32
                pitch f32
            }
            DisplayObjective SET_DISPLAY_OBJECTIVE {
                position VarInt
                score_name String
            }
            SetEntityMetadata SET_ENTITY_DATA {
                entity_id VarInt
                metadata EntityMetadata
            }
            LinkEntities SET_ENTITY_LINK {
                attached_entity_id i32
                holid_entity_id i32
            }
            SetEntityVelocity SET_ENTITY_MOTION {
                entity_id VarInt
                velocity LpVec3
            }
            SetEquipment SET_EQUIPMENT {
                entity_id VarInt
                equipment EntityEquipment
            }
            SetExperience SET_EXPERIENCE {
                experience_bar f32
                level VarInt
                total_experience VarInt
            }
            SetHealth SET_HEALTH {
                health f32
                food VarInt
                food_saturation f32
            }
            SetHeldItemClientbound SET_HELD_SLOT {
                slot VarInt
            }
            UpdateObjectives SET_OBJECTIVE {
                objective_name String
                mode ObjectiveMode
            }
            SetPassengers SET_PASSENGERS {
                entity_id VarInt
                passengers PrefixedArray<VarInt>
            }
            SetPlayerInventorySlot SET_PLAYER_INVENTORY {
                slot VarInt
                slot_data Slot
            }
            UpdateTeams SET_PLAYER_TEAM {
                team_name String
                method TeamMethod
            }
            UpdateScore SET_SCORE {
                entity_name String
                objective_name String
                value VarInt
                display_name Option<TextComponent>
                number_format Option<ObjectiveNumberFormat>
            }
            SetSimulationDistance SET_SIMULATION_DISTANCE {
                simulation_distance VarInt
            }
            SetSubtitleText SET_SUBTITLE_TEXT {
                subtitle_text TextComponent
            }
            UpdateTime SET_TIME {
                world_age i64
                clocks PrefixedArray<Clock>
            }
            SetTitleText SET_TITLE_TEXT {
                title_text TextComponent
            }
            SetTitleAnimationTimes SET_TITLES_ANIMATION {
                fade_in i32
                stay i32
                fade_out i32
            }
            EntitySoundEffect SOUND_ENTITY {
                sound_event IdOrX<SoundEvent>
                sound_category VarInt
                entity_id VarInt
                volume f32
                pitch f32
                seed i64
            }
            SoundEffect SOUND {
                sound_event IdOrX<SoundEvent>
                sound_category VarInt
                effect_position Vec3<i32>
                volume f32
                pitch f32
                seed i64
            }
            StartConfiguration START_CONFIGURATION {}
            StopSound STOP_SOUND {
                data StopSoundData
            }
            StoreCookiePlay STORE_COOKIE_PLAY {
                key Identifier
                payload LenPrefixedBytes<VarInt>
            }
            SystemChatMessage SYSTEM_CHAT {
                content TextComponent
                overlay bool
            }
            SetTabListHeaderAndFooter TAB_LIST {
                header TextComponent
                footer TextComponent
            }
            TagQueryResponse TAG_QUERY {
                transaction_id VarInt
                nbt nbt::Tag
            }
            PickupItem TAKE_ITEM_ENTITY {
                collceted_entity_id VarInt
                collector_entity_id VarInt
                pickup_item_count VarInt
            }
            SynchronizeVehiclePosition TELEPORT_ENTITY {
                entity_id VarInt
                position Vec3<f64>
                velocity Vec3<f64>
                yaw f32
                pitch f32
                flags TeleportFlags
                on_ground bool
            }
            TestInstanceBlockStatus TEST_INSTANCE_BLOCK_STATUS {
                stauts TextComponent
                size Option<Vec3<f64>>
            }
            SetTickingState TICKING_STATE {
                tick_rate f32
                is_frozen bool
            }
            StepTick TICKING_STEP {
                tick_steps VarInt
            }
            TransferPlay TRANSFER_PLAY {
                host String
                port VarInt
            }
            UpdateAdvancements UPDATE_ADVANCEMENTS {
                reset_clear bool
                advancement_mappings PrefixedArray<AdvancementMapping>
                identifiers PrefixedArray<Identifier>
                progress_mappings PrefixedArray<ProgressMapping>
                show_advancements bool
            }
            UpdateAttributes UPDATE_ATTRIBUTES {
                entity_id VarInt
                properties PrefixedArray<EntityProperty>
            }
            EntityEffect UPDATE_MOB_EFFECT {
                entity_id VarInt
                effect_id VarInt
                amplifier VarInt
                duration VarInt
                flags i8
            }
            UpdateRecipes UPDATE_RECIPES {
                property_sets PrefixedArray<PropertySet>
                stonecutter_recipes PrefixedArray<StonecutterRecipe>
            }
            UpdateTagsPlay UPDATE_TAGS_PLAY {
                registry_to_tags_map PrefixedArray<ReigstryToTags>
            }
            ProjectilePower PROJECTILE_POWER {
                entity_id VarInt
                power f64
            }
            CustomReportDetails CUSTOM_REPORT_DETAILS_PLAY {
                details PrefixedArray<CustomReportDetail>
            }
            ServerLinks SERVER_LINKS_PLAY {
                links PrefixedArray<ServerLink>
            }
            Waypoint WAYPOINT {
                operation VarInt // TODO: enum
                identifier XorY<UUID, String>
                icon_style Identifier
                color Option<(u8,u8,u8)>
                waypoint WaypointData
            }
            ClearDialogPlay CLEAR_DIALOG_PLAY {}
            ShowDialogPlay SHOW_DIALOG_PLAY {
                dialog IdOrX<nbt::Tag>
            }
        }
    }
}

#[derive(Serializable, Debug, Clone, Copy)]
#[enum_info(u8)]
pub enum Intent {
    #[enum_idx(1)]
    Status,
    Login,
    Transfer,
}

impl From<Intent> for State {
    fn from(val: Intent) -> Self {
        match val {
            Intent::Status => State::Status,
            Intent::Login => State::Login,
            Intent::Transfer => State::Configuration,
        }
    }
}

#[derive(Serializable, Debug)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Serializable, Debug)]
pub struct RegistryEntry {
    pub entry_id: Identifier,
    pub data: Option<nbt::Tag>,
}

#[derive(Serializable, Debug)]
pub struct Tags {
    pub registry: Identifier,
    pub tags: PrefixedArray<Tag>,
}

#[derive(Serializable, Debug)]
pub struct Tag {
    pub tag_name: Identifier,
    pub entries: PrefixedArray<VarInt>,
}

#[derive(Serializable, Debug, Clone)]
pub struct KnownPack {
    pub namespace: String,
    pub id: String,
    pub version: String,
}

#[derive(Serializable, Debug)]
pub struct ReportDetail {
    pub title: String,
    pub description: String,
}

#[derive(Serializable, Debug)]
pub struct ServerLink {
    pub label: LinkLabel,
    pub url: String,
}

#[derive(Serializable, Debug)]
#[enum_info(bool)]
pub enum LinkLabel {
    TextComponent(TextComponent),
    Label(LinkLabelEnum),
}

#[derive(Serializable, Debug)]
#[enum_info(VarInt)]
pub enum LinkLabelEnum {
    BugReport,
    CommunityGuidelines,
    Support,
    Status,
    Feedback,
    Community,
    Website,
    Forums,
    News,
    Announcements,
}

#[derive(Serializable, Debug)]
#[enum_info(VarInt)]
pub enum ChatMode {
    Enabled,
    CommandsOnly,
    Hidden,
}

#[derive(Serializable, Debug)]
#[bitfields(u8)]
pub struct SkinParts {
    pub cape: bool,
    pub jacket: bool,
    pub left_sleeve: bool,
    pub right_sleeve: bool,
    pub left_pants: bool,
    pub right_pants: bool,
    pub hat: bool,
}

#[derive(Serializable, Debug)]
#[enum_info(VarInt)]
pub enum MainHand {
    Left,
    Right,
}

#[derive(Serializable, Debug)]
#[enum_info(VarInt)]
pub enum ParticleStatus {
    All,
    Decreased,
    Minimal,
}

#[derive(Serializable, Debug)]
#[enum_info(VarInt)]
pub enum ResourcePackResult {
    SuccessfullyDownloaded,
    Declined,
    FailedToDownload,
    Accepted,
    Downloaded,
    InvalidUrl,
    FailedToReload,
    Discarded,
}

#[derive(Serializable, Debug)]
#[enum_info(u8)]
pub enum Animation {
    SwingMainArm,
    UNREACHABLE, // TODO: properly define this without derive macro
    LeaveBed,
    SwingOffhand,
    CriticalEffect,
    MagicCriticalEffect,
}

#[derive(Serializable, Debug)]
pub struct StatisticEntry {
    pub statistic: Statistic,
    pub value: VarInt,
}

#[derive(Serializable, Debug)]
#[enum_info(VarInt)]
pub enum Statistic {
    Mined { block: VarInt },
    Crafted { item: VarInt },
    Used { item: VarInt },
    Broken { item: VarInt },
    PickedUp { item: VarInt },
    Dropped { item: VarInt },
    Killed { entity: VarInt },
    KilledBy { entity: VarInt },
    Custom(CustomStatistic),
}

#[derive(Serializable, Debug)]
#[enum_info(VarInt)]
pub enum CustomStatistic {
    LeaveGame,
    PlayTime,
    TotalWorldTime,
    TimeSinceDeath,
    TimeSinceRest,
    SneakTime,
    WalkOneCm,
    CrouchOneCm,
    SprintOneCm,
    WalkOnWaterOneCm,
    FallOneCm,
    ClimbOneCm,
    FlyOneCm,
    WalkUnderWaterOneCm,
    MinecartOneCm,
    BoatOneCm,
    PigOneCm,
    HorseOneCm,
    AviateOneCm,
    SwimOneCm,
    StriderOneCm,
    Jump,
    Drop,
    DamageDealt,
    DamageDealtAbsorbed,
    DamageDealtResisted,
    DamageTaken,
    DamageBlockedByShield,
    DamageAbsorbed,
    DamageResisted,
    Deaths,
    MobKills,
    AnimalsBred,
    PlayerKills,
    FishCaught,
    TalkedToVillager,
    TradedWithVillager,
    EatCakeSlice,
    FillCauldron,
    UseCauldron,
    CleanArmor,
    CleanBanner,
    CleanShulkerBox,
    InteractWithBrewingstand,
    InteractWithBeacon,
    InspectDropper,
    InspectHopper,
    InspectDispenser,
    PlayNoteblock,
    TuneNoteblock,
    PotFlower,
    TriggerTrappedChest,
    OpenEnderchest,
    EnchantItem,
    PlayRecord,
    InteractWithFurnace,
    InteractWithCraftingTable,
    OpenChest,
    SleepInBed,
    OpenShulkerBox,
    OpenBarrel,
    InteractWithBlastFurnace,
    InteractWithSmoker,
    InteractWithLectern,
    InteractWithCampfire,
    InteractWithCartographyTable,
    InteractWithLoom,
    InteractWithStonecutter,
    BellRing,
    RaidTrigger,
    RaidWin,
    InteractWithAnvil,
    InteractWithGrindstone,
    TargetHit,
    InteractWithSmithingTable,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum BossAction {
    Add {
        title: TextComponent,
        health: f32,
        color: ColorId,
        division: DivisionType,
        flags: BossActionFlags,
    },
    Remove,
    UpdateHealth {
        health: f32,
    },
    UpdateTitle {
        title: TextComponent,
    },
    UpdateStyle {
        color: ColorId,
        dividers: ColorId,
    },
    UpdateFlags {
        flags: BossActionFlags,
    },
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ColorId {
    Pink,
    Blue,
    Red,
    Green,
    Yellow,
    Purple,
    White,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum DivisionType {
    NoDivision,
    SixNotches,
    TenNotches,
    TwelveNotches,
    TwentyNotches,
}

#[derive(Serializable, Debug)]
#[bitfields(u8)]
pub struct BossActionFlags {
    pub should_darken_sky: bool,
    pub is_dragon_bar: bool,
    pub create_fog: bool,
}

#[derive(Debug, Serializable)]
#[enum_info(u8)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
}

#[derive(Serializable, Debug)]
pub struct ChunkBiomeData {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub data: LenPrefixedBytes<VarInt>,
}

#[derive(Serializable, Debug)]
pub struct CommandSuggestionMatch {
    pub command_match: String,
    pub tooltip: Option<TextComponent>,
}

#[derive(Debug)]
pub struct Node {
    // read directly
    pub children: PrefixedArray<VarInt>,
    pub is_executable: bool,
    pub is_restricted: bool,
    // resolved
    pub redirect_node: Option<VarInt>,
    pub node_info: NodeInfo,
}

impl Serializable for Node {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        // read buf
        let flags = buf.read_u8()?;
        let children: PrefixedArray<VarInt> = Serializable::read_from(buf)?;

        // flags
        let node_type = flags & 0x03;
        let is_executable = flags & 0x04 != 0;
        let has_redirect = flags & 0x08 != 0;
        let has_suggestions_type = flags & 0x10 != 0;
        let is_restricted = flags & 0x20 != 0;

        // read buf
        let redirect_node = has_redirect
            .then(|| Serializable::read_from(buf))
            .transpose()?;

        let node_info = match node_type {
            0 => NodeInfo::Root,
            1 => NodeInfo::Literal {
                name: Serializable::read_from(buf)?,
            },
            2 => NodeInfo::Argument {
                name: Serializable::read_from(buf)?,
                parser: Serializable::read_from(buf)?,
                suggestions_type: has_suggestions_type
                    .then(|| Serializable::read_from(buf))
                    .transpose()?,
            },
            3 => NodeInfo::Root,
            _ => {
                return Err(ReadingError::Message(format!(
                    "invalid node type id: {}",
                    node_type
                )));
            }
        };

        let result = Node {
            is_executable,
            is_restricted,
            children,
            redirect_node,
            node_info,
        };

        Ok(result)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let node_type;
        let has_redirect = self.redirect_node.is_some();
        let mut has_suggestions_type = false;

        let mut node_name = None;
        let mut node_parser = None;
        let mut node_suggestions_type = &None;

        match &self.node_info {
            NodeInfo::Root => node_type = 0,
            NodeInfo::Literal { name } => {
                node_type = 1;
                node_name = Some(name);
            }
            NodeInfo::Argument {
                name,
                parser,
                suggestions_type,
            } => {
                node_type = 2;
                has_suggestions_type = suggestions_type.is_some();
                node_name = Some(name);
                node_parser = Some(parser);
                node_suggestions_type = suggestions_type;
            }
        };

        let mut flags: u8 = node_type;
        flags |= (self.is_executable as u8) << 2;
        flags |= (has_redirect as u8) << 3;
        flags |= (has_suggestions_type as u8) << 4;
        flags |= (self.is_restricted as u8) << 5;

        buf.write_u8(flags)?;
        self.children.write_to(buf)?;
        if let Some(val) = &self.redirect_node {
            val.write_to(buf)?;
        }
        if let Some(val) = node_name {
            val.write_to(buf)?;
        }
        if let Some(val) = node_parser {
            val.write_to(buf)?;
        }
        if let Some(val) = node_suggestions_type {
            val.write_to(buf)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum NodeInfo {
    Root,
    Literal {
        name: String,
    },
    Argument {
        name: String,
        parser: Parser,
        suggestions_type: Option<Identifier>,
    },
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum Parser {
    BrigadierBool,
    BrigadierFloat(BrigadierNumOptions<f32>),
    BrigadierDouble(BrigadierNumOptions<f64>),
    BrigadierInteger(BrigadierNumOptions<i32>),
    BrigadierLong(BrigadierNumOptions<i64>),
    BrigadierString(BrigadierStringOptions),
    MinecraftEntity(MinecraftEntityOptions),
    MinecraftGameProfile,
    MinecraftBlockPos,
    MinecraftColumnPos,
    MinecraftVec3,
    MinecraftVec2,
    MinecraftBlockState,
    MinecraftBlockPredicate,
    MinecraftItemStack,
    MinecraftItemPredicate,
    MinecraftColor,
    MinecraftHexColor,
    MinecraftComponent,
    MinecraftStyle,
    MinecraftMessage,
    MinecraftNbtCompoundTag,
    MinecraftNbtTag,
    MinecraftNbtPath,
    MinecraftObjective,
    MinecraftObjectiveCriteria,
    MinecraftOperation,
    MinecraftParticle,
    MinecraftAngle,
    MinecraftRotation,
    MinecraftScoreboardSlot,
    MinecraftScoreHolder(MinecraftScoreHolderOptions),
    MinecraftSwizzle,
    MinecraftTeam,
    MinecraftItemSlot,
    MinecraftItemSlots,
    MinecraftResourceLocation,
    MinecraftFunction,
    MinecraftEntityAnchor,
    MinecraftIntRange,
    MinecraftFloatRange,
    MinecraftDimension,
    MinecraftGamemode,
    MinecraftTime(MinecraftTimeOptions),
    MinecraftResourceOrTag(MinecraftResourceOrTagOptions),
    MinecraftResourceOrTagKey(MinecraftResourceOrTagKeyOptions),
    MinecraftResource(MinecraftResourceOptions),
    MinecraftResourceKey(MinecraftResourceKeyOptions),
    MinecraftResourceSelector(MinecraftResourceSelectorOptions),
    MinecraftTemplateMirror,
    MinecraftTemplateRotation,
    MinecraftHeightmap,
    MinecraftLootTable,
    MinecraftLootPredicate,
    MinecraftLootModifier,
    MinecraftDialog,
    MinecraftUuid,
}

pub trait Bounded {
    const MIN: Self;
    const MAX: Self;
}

impl Bounded for f32 {
    const MIN: Self = Self::MIN;
    const MAX: Self = Self::MAX;
}

impl Bounded for f64 {
    const MIN: Self = Self::MIN;
    const MAX: Self = Self::MAX;
}

impl Bounded for i32 {
    const MIN: Self = Self::MIN;
    const MAX: Self = Self::MAX;
}

impl Bounded for i64 {
    const MIN: Self = Self::MIN;
    const MAX: Self = Self::MAX;
}

#[derive(Debug)]
pub struct BrigadierNumOptions<T: Serializable + Bounded + PartialEq + Copy> {
    pub min: T,
    pub max: T,
}

impl<T: Serializable + Bounded + PartialEq + Copy> Serializable for BrigadierNumOptions<T> {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let flags = buf.read_u8()?;

        let min = if flags & 0x01 != 0 {
            Serializable::read_from(buf)?
        } else {
            T::MIN
        };

        let max = if flags & 0x02 != 0 {
            Serializable::read_from(buf)?
        } else {
            T::MAX
        };
        Ok(BrigadierNumOptions { min, max })
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut flags = 0u8;
        let mut min = None;
        let mut max = None;
        if self.min != T::MIN {
            flags |= 1;
            min = Some(self.min);
        }
        if self.max != T::MAX {
            flags |= 1 << 1;
            max = Some(self.max);
        }

        buf.write_u8(flags)?;

        if let Some(val) = min {
            val.write_to(buf)?;
        }
        if let Some(val) = max {
            val.write_to(buf)?;
        }

        Ok(())
    }
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum BrigadierStringOptions {
    SingleWord,
    QuotablePhrase,
    GreedyPhrase,
}

#[derive(Debug, Serializable)]
#[bitfields(u8)]
pub struct MinecraftEntityOptions {
    pub single_entity: bool,
    pub player_only: bool,
}

#[derive(Debug, Serializable)]
#[bitfields(u8)]
pub struct MinecraftScoreHolderOptions {
    pub multiple: bool,
}

#[derive(Debug, Serializable)]
pub struct MinecraftTimeOptions {
    pub min: i32,
}

#[derive(Debug, Serializable)]
pub struct MinecraftResourceOrTagOptions {
    pub registry: Identifier,
}

#[derive(Debug, Serializable)]
pub struct MinecraftResourceOrTagKeyOptions {
    pub registry: Identifier,
}

#[derive(Debug, Serializable)]
pub struct MinecraftResourceOptions {
    pub registry: Identifier,
}

#[derive(Debug, Serializable)]
pub struct MinecraftResourceKeyOptions {
    pub registry: Identifier,
}

#[derive(Debug, Serializable)]
pub struct MinecraftResourceSelectorOptions {
    pub registry: Identifier,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ChatSuggestionAction {
    Add,
    Remove,
    Set,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum DebugSampleType {
    TickTime,
}

#[derive(Debug, Serializable)]
pub struct ChatType {
    pub chat: ChatTypeDecorations,
    pub narration: ChatTypeDecorations,
}

#[derive(Debug, Serializable)]
pub struct ChatTypeDecorations {
    pub translartion_key: String,
    pub parameters: PrefixedArray<ChatTypeParameters>,
    pub style: nbt::Tag,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ChatTypeParameters {
    Sender,
    Target,
    Content,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum Particle {
    AngryVillager,
    Block {
        block_state: VarInt,
    },
    BlockMarker {
        block_state: VarInt,
    },
    Bubble,
    Cloud,
    Crit,
    DamageIndicator,
    DragonBreath {
        power: f32,
    },
    DrippingLava,
    FallingLava,
    LandingLava,
    DrippingWater,
    FallingWater,
    Dust {
        color: ColorI32,
        scale: f32,
    },
    DustColorTransition {
        from_color: ColorI32,
        to_color: ColorI32,
        scale: f32,
    },
    Effect {
        color: ColorI32,
        power: f32,
    },
    ElderGuardian,
    EnchantedHit,
    Enchant,
    EndRod,
    EntityEffect {
        color: ColorARGBI32,
    },
    ExplosionEmitter,
    Explosion,
    Gust,
    SmallGust,
    GustEmitterLarge,
    GustEmitterSmall,
    SonicBoom,
    FallingDust {
        block_state: VarInt,
    },
    Firework,
    Fishing,
    Flame,
    Infested,
    CherryLeaves,
    PaleOakLeaves,
    TintedLeaves {
        color: ColorARGBI32,
    },
    SculkSoul,
    SculkCharge {
        roll: f32,
    },
    SculkChargePop,
    SoulFireFlame,
    Soul,
    Flash {
        color: ColorARGBI32,
    },
    HappyVillager,
    Composter,
    Heart,
    InstantEffect {
        color: ColorI32,
        power: f32,
    },
    Item(Slot),
    Vibration {
        vibration_data: VibrationData,
        ticks: VarInt,
    },
    Trail {
        target: Vec3<f64>,
        color: ColorI32,
        duration: VarInt,
    },
    ItemSlime,
    ItemCobweb,
    ItemSnowball,
    LargeSmoke,
    Lava,
    Mycelium,
    Note,
    Poof,
    Portal,
    Rain,
    Smoke,
    WhiteSmoke,
    Sneeze,
    Spit,
    SquidInk,
    SweepAttack,
    TotemOfUndying,
    Underwater,
    Splash,
    Witch,
    BubblePop,
    CurrentDown,
    BubbleColumnUp,
    Nautilus,
    Dolphin,
    CampfireCosySmoke,
    CampfireSignalSmoke,
    DrippingHoney,
    FallingHoney,
    LandingHoney,
    FallingNectar,
    FallingSporeBlossom,
    Ash,
    CrimsonSpore,
    WarpedSpore,
    SporeBlossomAir,
    DrippingObsidianTear,
    FallingObsidianTear,
    LandingObsidianTear,
    ReversePortal,
    WhiteAsh,
    SmallFlame,
    Snowflake,
    DrippingDripstoneLava,
    FallingDripstoneLava,
    DrippingDripstoneWater,
    FallingDripstoneWater,
    GlowSquidInk,
    Glow,
    WaxOn,
    WaxOff,
    ElectricSpark,
    Scrape,
    Shriek {
        delay: VarInt,
    },
    EggCrack,
    DustPlume,
    TrialSpawnerDetection,
    TrialSpawnerDetectionOminous,
    VaultConnection,
    DustPillar {
        block_state: VarInt,
    },
    OminousSpawning,
    RaidOmen,
    TrialOmen,
    BlockCrumble {
        block_state: VarInt,
    },
    Firefly,
    SulfurCubeGoo,
}

#[derive(Debug)]
pub struct ColorARGBI32 {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Serializable for ColorARGBI32 {
    fn read_from<R: std::io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let int = buf.read_u32::<BigEndian>()?;
        let a = (int >> 24) as u8;
        let r = (int >> 16) as u8;
        let g = (int >> 8) as u8;
        let b = int as u8;
        Ok(ColorARGBI32 { a, r, g, b })
    }
    fn write_to<W: std::io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut int = self.b as u32;
        int |= (self.g as u32) << 8;
        int |= (self.r as u32) << 16;
        int |= (self.a as u32) << 24;
        buf.write_u32::<BigEndian>(int)?;
        Ok(())
    }
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum VibrationData {
    Block {
        block_position: Position,
    },
    Entity {
        entity_id: VarInt,
        entity_eye_height: f32,
    },
}

#[derive(Debug, Serializable)]
pub struct BlockEntity {
    pub packed_xz: PackedXZ,
    pub y: i16,
    pub ty: VarInt,
    pub data: nbt::Tag,
}

#[derive(Debug)]
pub struct PackedXZ {
    x: u8,
    z: u8,
}

impl Serializable for PackedXZ {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let packed_xz = buf.read_u8()?;
        let x = packed_xz >> 4;
        let z = packed_xz & 15;
        Ok(PackedXZ { x, z })
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let packed_xz = ((self.x & 15) << 4) | (self.z & 15);
        buf.write_u8(packed_xz)?;
        Ok(())
    }
}

#[derive(Debug, Serializable)]
pub struct HeightMap {
    pub ty: HeightMapType,
    pub data: PrefixedArray<i64>,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum HeightMapType {
    #[enum_idx(1)]
    WorldSurface,
    #[enum_idx(4)]
    MotionBlocking,
    MotionBlockingNoLeaves,
}

#[derive(Debug, Serializable)]
pub struct LightData {
    pub sky_light_mask: BitSet,
    pub block_light_mask: BitSet,
    pub empty_sky_light_mask: BitSet,
    pub empty_block_light_mask: BitSet,
    pub sky_light_arrays: PrefixedArray<LenPrefixedBytes<VarInt>>,
    pub block_light_arrays: PrefixedArray<LenPrefixedBytes<VarInt>>,
}

#[derive(Debug, Serializable)]
pub struct DeathInfo {
    pub death_dimension_name: Identifier,
    pub death_location: Position,
}

#[derive(Debug, Serializable)]
pub struct MapIcon {
    pub ty: VarInt,
    pub x: i8,
    pub z: i8,
    pub direction: i8,
    pub display_name: Option<TextComponent>,
}

#[derive(Debug)]
pub enum MapColorPatch {
    NoColumns,
    HasColumns {
        columns: u8,
        rows: u8,
        x: u8,
        z: u8,
        data: LenPrefixedBytes<u8>,
    },
}

impl Serializable for MapColorPatch {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let columns = buf.read_u8()?;
        let ret = match columns {
            0 => Self::NoColumns,
            n => Self::HasColumns {
                columns: n,
                rows: Serializable::read_from(buf)?,
                x: Serializable::read_from(buf)?,
                z: Serializable::read_from(buf)?,
                data: Serializable::read_from(buf)?,
            },
        };
        Ok(ret)
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        match self {
            Self::NoColumns => buf.write_u8(0)?,
            Self::HasColumns {
                columns,
                rows,
                x,
                z,
                data,
            } => {
                columns.write_to(buf)?;
                rows.write_to(buf)?;
                x.write_to(buf)?;
                z.write_to(buf)?;
                data.write_to(buf)?;
            }
        };
        Ok(())
    }
}

#[derive(Debug, Serializable)]
pub struct MerchantTrade {
    pub input_item_1: TradeItem,
    pub output_item: Slot,
    pub input_item_2: Option<TradeItem>,
    pub trade_disabled: bool,
    pub number_of_trade_uses: i32,
    pub max_number_of_trade_uses: i32,
    pub xp: i32,
    pub special_price: i32,
    pub price_multiplier: f32,
    pub demand: i32,
}

#[derive(Debug, Serializable)]
pub struct TradeItem {
    pub item_id: VarInt,
    pub item_count: VarInt,
    pub components: PrefixedArray<Component>,
}

#[derive(Debug, Serializable)]
pub struct MinecartStep {
    pub pos: Vec3<f64>,
    pub velocity: Vec3<f64>,
    pub yaw: Angle,
    pub pitch: Angle,
    pub weight: f32,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum Hand {
    Main,
    Offhand,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum RecipeDisplay {
    CraftingShapeless {
        ingredients: PrefixedArray<SlotDisplay>,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    CraftingShaped {
        width: VarInt,
        height: VarInt,
        ingredients: PrefixedArray<SlotDisplay>,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    Furnace {
        ingredient: SlotDisplay,
        fuel: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
        cooking_time: VarInt,
        experience: f32,
    },
    Stonecutter {
        ingredient: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
    Smithing {
        template: SlotDisplay,
        base: SlotDisplay,
        addition: SlotDisplay,
        result: SlotDisplay,
        crafting_station: SlotDisplay,
    },
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum SlotDisplay {
    Empty,
    AnyFuel,
    WithAnyPotion(Box<SlotDisplay>),
    OnlyWithComponent {
        base: Box<SlotDisplay>,
        component_type_id: VarInt,
    },
    Item(VarInt),
    ItemStack(Slot),
    Tag(Identifier),
    Dyed {
        dye: Box<SlotDisplay>,
        target: Box<SlotDisplay>,
    },
    SmithingTrim {
        base: Box<SlotDisplay>,
        material: Box<SlotDisplay>,
        pattern: Box<SlotDisplay>,
    },
    WithRemainder {
        ingredient: Box<SlotDisplay>,
        remainder: Box<SlotDisplay>,
    },
    Composite {
        options: PrefixedArray<SlotDisplay>,
    },
}

#[derive(Debug, Serializable)]
#[bitfields(u8)]
pub struct PlayerAbilitiesFlags {
    pub invulnerable: bool,
    pub flying: bool,
    pub allow_flying: bool,
    pub creative_mode: bool,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ChatMessageFilterType {
    PassThrough,
    FullyFiltered,
    PartiallyFiltered { filter_type_bits: BitSet },
}

#[derive(Debug)]
pub struct PlayersActionsData {
    // pub actions: FixedBitSet<1>,
    // LEN PREFIXED by varint
    pub players_actions: Vec<PlayerActions>,
}

// TODO: when std::mem::variant_count() is stabilized make a type for enumset
impl Serializable for PlayersActionsData {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        // this specific enum has 8 variants
        let actions = FixedBitSet::<8>::read_from(buf)?;

        let len = VarInt::read_from(buf)?.into_len();

        let mut players: Vec<PlayerActions> = Vec::with_capacity(len);
        for _ in 0..len {
            let uuid = UUID::read_from(buf)?;
            let mut player_actions: Vec<PlayerAction> = Vec::new();
            for i in 0..8 {
                if actions.get(i) {
                    let action = match i {
                        0 => PlayerAction::AddPlayer {
                            name: Serializable::read_from(buf)?,
                            properties: Serializable::read_from(buf)?,
                        },
                        1 => PlayerAction::InitializeChat {
                            data: Serializable::read_from(buf)?,
                        },

                        2 => PlayerAction::UpdateGamemode {
                            gamemode: Serializable::read_from(buf)?,
                        },
                        3 => PlayerAction::UpdateListed {
                            listed: Serializable::read_from(buf)?,
                        },
                        4 => PlayerAction::UpdateLatency {
                            ping: Serializable::read_from(buf)?,
                        },
                        5 => PlayerAction::UpdateDisplayName {
                            display_name: Serializable::read_from(buf)?,
                        },
                        6 => PlayerAction::UpdateListPriority {
                            priority: Serializable::read_from(buf)?,
                        },

                        7 => PlayerAction::UpdateHat {
                            visible: Serializable::read_from(buf)?,
                        },
                        _ => unreachable!(),
                    };
                    player_actions.push(action);
                }
            }

            players.push(PlayerActions {
                uuid,
                player_actions,
            });
        }

        Ok(PlayersActionsData {
            players_actions: players,
        })
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        // this specific enum has 8 variants
        let mut actions = FixedBitSet::<8>::new();

        for player_actions in &self.players_actions {
            for player_action in &player_actions.player_actions {
                let index = match player_action {
                    PlayerAction::AddPlayer {
                        name: _,
                        properties: _,
                    } => 0,
                    PlayerAction::InitializeChat { data: _ } => 1,
                    PlayerAction::UpdateDisplayName { display_name: _ } => 2,
                    PlayerAction::UpdateGamemode { gamemode: _ } => 3,
                    PlayerAction::UpdateHat { visible: _ } => 4,
                    PlayerAction::UpdateLatency { ping: _ } => 5,
                    PlayerAction::UpdateListPriority { priority: _ } => 6,
                    PlayerAction::UpdateListed { listed: _ } => 7,
                };
                actions.set(index);
            }
        }

        actions.write_to(buf)?;
        let len = self.players_actions.len();
        VarInt::from_len(len).write_to(buf)?;
        for player_actions in &self.players_actions {
            player_actions.uuid.write_to(buf)?;
            for player_action in &player_actions.player_actions {
                match player_action {
                    PlayerAction::AddPlayer { name, properties } => {
                        name.write_to(buf)?;
                        properties.write_to(buf)?;
                    }
                    PlayerAction::InitializeChat { data } => data.write_to(buf)?,

                    PlayerAction::UpdateDisplayName { display_name } => {
                        display_name.write_to(buf)?
                    }

                    PlayerAction::UpdateGamemode { gamemode } => gamemode.write_to(buf)?,

                    PlayerAction::UpdateHat { visible } => visible.write_to(buf)?,

                    PlayerAction::UpdateLatency { ping } => ping.write_to(buf)?,

                    PlayerAction::UpdateListPriority { priority } => priority.write_to(buf)?,

                    PlayerAction::UpdateListed { listed } => listed.write_to(buf)?,
                };
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct PlayerActions {
    pub uuid: UUID,
    pub player_actions: Vec<PlayerAction>,
}

#[derive(Debug)]
pub enum PlayerAction {
    AddPlayer {
        name: String,
        properties: PrefixedArray<ProfileProperty>,
    },
    InitializeChat {
        data: Option<InitializeChatData>,
    },
    UpdateGamemode {
        gamemode: VarInt,
    },
    UpdateListed {
        listed: bool,
    },
    UpdateLatency {
        ping: VarInt,
    },
    UpdateDisplayName {
        display_name: Option<TextComponent>,
    },
    UpdateListPriority {
        priority: VarInt,
    },
    UpdateHat {
        visible: bool,
    },
}

#[derive(Debug, Serializable)]
pub struct InitializeChatData {
    pub chat_session_id: UUID,
    pub public_key_expire_time: i64,
    pub enocoded_public_key: LenPrefixedBytes<VarInt>,
    pub public_key_signature: LenPrefixedBytes<VarInt>,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum FeetEyes {
    Feet,
    Eyes,
}

#[derive(Debug, Serializable)]
pub struct LookAtEntityInfo {
    pub entity_id: VarInt,
    pub feet_eyes: FeetEyes,
}

#[derive(Debug, Serializable)]
#[bitfields(i32)]
pub struct TeleportFlags {
    pub relative_x: bool,
    pub relative_y: bool,
    pub relative_z: bool,
    pub relative_yaw: bool,
    pub relative_pitch: bool,
    pub relative_velocity_x: bool,
    pub relative_velocity_y: bool,
    pub relative_velocity_z: bool,
    pub rotate_velocity_accoridng_to_rotation: bool,
}

#[derive(Debug, Serializable)]
pub struct Recipe {
    pub recipe_id: VarInt,
    pub display: RecipeDisplay,
    pub group_id: VarInt,
    pub category_id: VarInt,
    pub ingredients: Option<PrefixedArray<IdSet>>,
    pub flags: RecipeFlags,
}

#[derive(Debug, Serializable)]
#[bitfields(u8)]
pub struct RecipeFlags {
    pub show_notification: bool,
    pub highlight_as_new: bool,
}

#[derive(Debug, Serializable)]
#[bitfields(u8)]
pub struct DataKept {
    pub keep_atributes: bool,
    pub keep_metadata: bool,
}

#[derive(Debug)]
pub struct EntityMetadata(Vec<EntityMetadatum>);

impl Serializable for EntityMetadata {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let mut entity_metadata = Vec::new();

        loop {
            let index = buf.read_u8()?;

            if index == 0xff {
                break;
            }

            let value = EntityMetadatumValue::read_from(buf)?;

            entity_metadata.push(EntityMetadatum { index, value });
        }

        Ok(Self(entity_metadata))
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        for entity_metadatum in &self.0 {
            entity_metadatum.write_to(buf)?;
        }
        buf.write_u8(0xff)?;
        Ok(())
    }
}

#[derive(Debug, Serializable)]
pub struct EntityMetadatum {
    pub index: u8,
    pub value: EntityMetadatumValue,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum EntityMetadatumValue {
    Byte(i8),
    VarInt(VarInt),
    VarLong(VarLong),
    Float(f32),
    String(String),
    TextComponent(TextComponent),
    OptionalTextComponent(Option<TextComponent>),
    Slot(Slot),
    Boolean(bool),
    Rotations(f32, f32, f32),
    Position(Position),
    OptionalPosition(Option<Position>),
    Direction(VarInt),
    OptionalLivingEntityReference(Option<UUID>),
    BlockState(VarInt),
    OptionalBlockState(VarInt),
    Particle(Particle),
    Particles(PrefixedArray<Particle>),
    VillagerData(VarInt, VarInt, VarInt),
    OptionalVarInt(IdOrX<()>),
    Pose(VarInt),
    CatVariant(VarInt),
    CatSoundVariant(VarInt),
    CowVariant(VarInt),
    CowSoundVariant(VarInt),
    WolfVariant(VarInt),
    WolfSoundVariant(VarInt),
    FrogVariant(VarInt),
    PigVariant(VarInt),
    PigSoundVariant(VarInt),
    ChickenVariant(VarInt),
    ChickenSoundVariant(VarInt),
    ZombieNautilus(VarInt),
    OptionalGlobalPosition(Option<GlobalPosition>),
    PaintingVariant(IdOrX<PaintingVariant>),
    SnifferState(VarInt),
    ArmadilloState(VarInt),
    CopperGolemState(VarInt),
    WeatheringCopperState(VarInt),
    Vec3(Vec3<f32>),
    Quaternion(Vec4<f32>),
    ResolvableProfile(ResolvableProfile),
    /// ENUM: 0 = LEFT 1= RIGHT todo
    HumanoidArm(VarInt),
}

#[derive(Debug, Serializable)]
pub struct GlobalPosition {
    pub identifier: Identifier,
    pub position: Position,
}

#[derive(Debug)]
pub struct EntityEquipment {
    pub equipment: Vec<EquipmentEntry>,
}

impl Serializable for EntityEquipment {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let mut equipment: Vec<EquipmentEntry> = Vec::new();

        loop {
            let slot = u8::read_from(buf)?;

            let equipment_slot = EquipmentSlot::read_from(&mut &[slot][..])?;

            let item = Slot::read_from(buf)?;

            equipment.push(EquipmentEntry {
                slot: equipment_slot,
                item,
            });

            if slot & 0x80 == 0 {
                return Ok(EntityEquipment { equipment });
            }
        }
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut iter = self.equipment.iter().peekable();
        while let Some(equipment_entry) = iter.next() {
            let mut byte = [0u8; 1];
            equipment_entry.slot.write_to(&mut byte.as_mut_slice())?;
            if iter.peek().is_some() {
                byte[0] |= 0x80
            }
            buf.write_u8(byte[0])?;
            equipment_entry.item.write_to(buf)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct EquipmentEntry {
    pub slot: EquipmentSlot,
    pub item: Slot,
}

#[derive(Debug, Serializable)]
#[enum_info(i8)]
pub enum EquipmentSlot {
    MainHand,
    Offhand,
    Boots,
    Leggings,
    Chestplate,
    Helmet,
    Body,
    Saddle,
}

#[derive(Debug, Serializable)]
#[enum_info(i8)]
pub enum ObjectiveMode {
    Create(ObjectiveData),
    Remove,
    UpdateDisplayText(ObjectiveData),
}

#[derive(Debug, Serializable)]
pub struct ObjectiveData {
    pub objective_value: TextComponent,
    pub ty: ObjectiveType,
    pub number_format: Option<ObjectiveNumberFormat>,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ObjectiveType {
    Integer,
    Hearts,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ObjectiveNumberFormat {
    Blank,
    Styled { styling: nbt::Tag },
    Fixed { content: TextComponent },
}

#[derive(Debug, Serializable)]
#[enum_info(i8)]
pub enum TeamMethod {
    Create {
        info: TeamInfo,
        entities: PrefixedArray<String>,
    },
    Remove,
    UpdateInfo(TeamInfo),
    AddEntities(PrefixedArray<String>),
    RemoveEntities(PrefixedArray<String>),
}

#[derive(Debug, Serializable)]
pub struct TeamInfo {
    pub team_display_name: TextComponent,
    pub friendly_flags: TeamFriendlyFlags,
    pub name_tag_visibility: VarInt,
    pub collision_rule: VarInt,
    pub team_color: VarInt,
    pub team_prefix: TextComponent,
    pub team_suffix: TextComponent,
}

#[derive(Debug, Serializable)]
#[bitfields(u8)]
pub struct TeamFriendlyFlags {
    pub allow_friendly_fire: bool,
    pub can_see_invisible_players: bool,
}

#[derive(Debug)]
pub struct StopSoundData {
    pub source: Option<VarInt>,
    pub sound: Option<Identifier>,
}

impl Serializable for StopSoundData {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let mut source = None;
        let mut sound = None;

        let flags = buf.read_u8()?;
        if flags == 1 || flags == 3 {
            source = Serializable::read_from(buf)?;
        }
        if flags == 2 || flags == 3 {
            sound = Serializable::read_from(buf)?;
        }
        Ok(StopSoundData { source, sound })
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut flags = 0u8;
        if self.source.is_some() {
            flags |= 1
        };
        if self.sound.is_some() {
            flags |= 1 << 1
        };
        buf.write_u8(flags)?;
        if let Some(val) = &self.source {
            val.write_to(buf)?;
        }
        if let Some(val) = &self.sound {
            val.write_to(buf)?;
        }
        Ok(())
    }
}

#[derive(Debug, Serializable)]
pub struct AdvancementMapping {
    pub key: Identifier,
    pub value: Advancement,
}

#[derive(Debug, Serializable)]
pub struct Advancement {
    pub parent_id: Option<Identifier>,
    pub display_data: Option<AdvancementDisplay>,
    pub nested_requirements: PrefixedArray<PrefixedArray<String>>,
    pub sends_telemetry_data: bool,
}

#[derive(Debug, Serializable)]
pub struct AdvancementDisplay {
    pub title: TextComponent,
    pub description: TextComponent,
    pub icon: Slot,
    pub frame_type: VarInt,
    pub flags: AdvancementDisplayFlags,
    pub x_coord: f32,
    pub y_coord: f32,
}

#[derive(Debug)]
pub struct AdvancementDisplayFlags {
    pub flags: i32,
    pub background_texture: Option<Identifier>,
}

impl Serializable for AdvancementDisplayFlags {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let flags = buf.read_i32::<BigEndian>()?;
        let mut background_texture = None;
        if flags & 0x01 == 1 {
            background_texture = Some(Identifier::read_from(buf)?);
        }
        Ok(AdvancementDisplayFlags {
            flags,
            background_texture,
        })
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        self.flags.write_to(buf)?;
        if let Some(val) = &self.background_texture {
            val.write_to(buf)?;
        }
        Ok(())
    }
}

#[derive(Debug, Serializable)]
pub struct ProgressMapping {
    pub key: Identifier,
    pub value: AdvancementProgress,
}

#[derive(Debug, Serializable)]
pub struct AdvancementProgress {
    pub crtieria: PrefixedArray<AdvancementProgressCriterion>,
}

#[derive(Debug, Serializable)]
pub struct AdvancementProgressCriterion {
    pub identifier: Identifier,
    ///number of milliseconds since January 1, 1970, 00:00:00 GMT
    pub date_of_achieving: Option<i64>,
}

#[derive(Debug, Serializable)]
pub struct EntityProperty {
    pub id: VarInt,
    pub value: f64,
    pub modifiers: PrefixedArray<ModifierData>,
}

#[derive(Debug, Serializable)]
pub struct ModifierData {
    pub id: Identifier,
    pub amount: f64,
    pub operation: i8,
}

#[derive(Debug, Serializable)]
pub struct PropertySet {
    pub id: Identifier,
    pub items: PrefixedArray<VarInt>,
}

#[derive(Debug, Serializable)]
pub struct StonecutterRecipe {
    pub ingredients: IdSet,
    pub slot_display: SlotDisplay,
}

#[derive(Debug, Serializable)]
pub struct ReigstryToTags {
    pub registry: Identifier,
    pub tags: PrefixedArray<Tag>,
}

#[derive(Debug, Serializable)]
pub struct CustomReportDetail {
    pub title: String,
    pub description: String,
}

#[derive(Debug, Serializable)]
pub struct ArgumentSignature {
    pub argument_name: String,
    pub signature: StaticLenBytes<256>,
}

#[derive(Debug, Serializable)]
pub struct ChangedSlot {
    pub slot_number: i16,
    pub slot_data: HashedSlot,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum InteractionType {
    Interact,
    Attack,
    InteractAt { target: Vec3<f32>, hand: Hand },
}

#[derive(Debug, Serializable)]
#[bitfields(u8)]
pub struct MovePlayerFlags {
    pub on_ground: bool,
    pub pushing_against_wall: bool,
}

#[derive(Debug, Serializable)]
#[bitfields(u8)]
pub struct PlayerInput {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sneak: bool,
    pub sprint: bool,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum SeenAdvancementsAction {
    OpenedTab { tab_id: Identifier },
    ClosedScreen,
}

#[derive(Debug, Serializable)]
pub struct GameProfile {
    pub uuid: UUID,
    pub username: String,
    pub properties: PrefixedArray<ProfileProperty>,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum DebugSubscriptionUpdate {
    DedicatedServerTickTime,
    Bee(DebugBeeData),
    VillagerBrain(VillagerBrianDebugData),
    Breeze(BreezeDebugData),
    GoalSelector(GoalSelectorDebugData),
    EntityPath(EntityPathDebugData),
    EntityBlockIntersection(EntityBlockIntersectionDebugData),
    BeeHive(BeeHiveDebugData),
    Poi(PoiDebugData),
    RedstoneWireOrientation(RedstoneWireOrientationDebugData),
    VillageSection,
    Raid(RaidDebugData),
    Structure(StructureDebugData),
    GameEventListener(GameEventListenerDebugData),
    NeighborUpdate(NeighborUpdateDebugData),
    GameEvent(GameEventDebugData),
}

#[derive(Debug, Serializable)]
pub struct DebugBeeData {
    pub hive_pos: Option<Position>,
    pub flower_pos: Option<Position>,
    pub travel_ticks: VarInt,
    pub blacklisted_hives: PrefixedArray<Position>,
}

#[derive(Debug, Serializable)]
pub struct VillagerBrianDebugData {
    pub name: String,
    pub profession: String,
    pub xp: i32,
    pub health: f32,
    pub max_health: f32,
    pub inventory: String,
    pub wants_golem: bool,
    pub anger_level: i32,
    pub activities: PrefixedArray<String>,
    pub behaviors: PrefixedArray<String>,
    pub memories: PrefixedArray<String>,
    pub gossips: PrefixedArray<String>,
    pub pois: PrefixedArray<Position>,
    pub potential_pois: PrefixedArray<Position>,
}

#[derive(Debug, Serializable)]
pub struct BreezeDebugData {
    pub attack_target: Option<VarInt>,
    pub jump_target: Option<VarInt>,
}

#[derive(Debug, Serializable)]
pub struct GoalSelectorDebugData {
    pub priority: VarInt,
    pub is_running: bool,
    pub name: String,
}

#[derive(Debug, Serializable)]
pub struct EntityPathDebugData {
    pub reached: bool,
    pub next_block_index: i32,
    pub block_pos: Position,
    pub nodes: PrefixedArray<DebugPathNode>,
    pub target_nodes: PrefixedArray<DebugPathNode>,
    pub open_set: PrefixedArray<DebugPathNode>,
    pub closed_set: PrefixedArray<DebugPathNode>,
    pub max_node_distance: f32,
}

#[derive(Debug, Serializable)]
pub struct DebugPathNode {
    pub pos: Vec3<i32>,
    pub walked_distance: f32,
    pub cost_malus: f32,
    pub closed: bool,
    pub ty: VarInt, // TODO: turn to enum
    pub f: f32,
}

#[derive(Debug, Serializable)]
pub struct EntityBlockIntersectionDebugData {
    pub id: VarInt, // TODO: enum
}

#[derive(Debug, Serializable)]
pub struct BeeHiveDebugData {
    pub ty: VarInt,
    pub occupant_count: VarInt,
    pub honey_level: VarInt,
    pub sedated: bool,
}

#[derive(Debug, Serializable)]
pub struct PoiDebugData {
    pub position: Position,
    pub ty: VarInt,
    pub free_ticket_count: VarInt,
}

#[derive(Debug, Serializable)]
pub struct RedstoneWireOrientationDebugData {
    pub id: VarInt,
}

#[derive(Debug, Serializable)]
pub struct RaidDebugData {
    pub positions: PrefixedArray<Position>,
}

#[derive(Debug, Serializable)]
pub struct StructureDebugData {
    pub structures: PrefixedArray<DebugStructureInfo>,
}

#[derive(Debug, Serializable)]
pub struct DebugStructureInfo {
    pub bounding_box_min: Position,
    pub bounding_box_max: Position,
    pub pieces: PrefixedArray<StructurePiece>,
}

#[derive(Debug, Serializable)]
pub struct StructurePiece {
    pub bounding_box_min: Position,
    pub bounding_box_max: Position,
    pub is_start: bool,
}

#[derive(Debug, Serializable)]
pub struct GameEventListenerDebugData {
    pub listener_radius: VarInt,
}

#[derive(Debug, Serializable)]
pub struct NeighborUpdateDebugData {
    pub position: Position,
}

#[derive(Debug, Serializable)]
pub struct GameEventDebugData {
    pub event: VarInt,
    pub pos: Vec3<f64>,
}

#[derive(Debug, Serializable)]
pub struct BlockParticleAlternative {
    pub particle: Particle,
    pub scaling: f32,
    pub speed: f32,
    pub weight: VarInt,
}

#[derive(Debug, Serializable)]
pub struct ResolvableProfile {
    pub unpack: ResolvableProfileUnpack,
    // TODO: for the following 4 wikivg says "Optional", not "Prefixed Optional". Investigate
    pub body: Option<Identifier>,
    pub cape: Option<Identifier>,
    pub elytra: Option<Identifier>,
    pub model: Option<VarInt>, // TODO: enum
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ResolvableProfileUnpack {
    Partial {
        username: Option<String>,
        uuid: Option<UUID>,
        properties: PrefixedArray<ProfileProperty>,
    },
    Complete(GameProfile),
}

#[derive(Debug, Serializable, PartialEq, Eq, Hash, Clone)]
#[enum_info(bool)]
pub enum XorY<X: Serializable, Y: Serializable> {
    Y(Y),
    X(X),
}

#[derive(Debug, Serializable, Clone)]
#[enum_info(VarInt)]
pub enum WaypointData {
    Empty,
    Vec3i(Vec3<VarInt>),
    Chunk { x: VarInt, z: VarInt },
    Azimuth { angle: f32 },
}

// todo: gotta rename this to a normal name, also check if it actually works cause i never tried it
#[derive(Debug)]
pub struct LpVec3(pub Vec3<f64>);

impl LpVec3 {
    fn clamp(val: f64, min: f64, max: f64) -> f64 {
        if val > max {
            return max;
        }
        if val < min {
            return min;
        }
        val
    }

    fn has_fast_marker_bit(max_directional_velocity: u32) -> bool {
        (max_directional_velocity & 4) == 4
    }

    fn clamp_value(value: f64) -> f64 {
        if value.is_nan() {
            0.0
        } else {
            Self::clamp(value, -1.7179869183E10, 1.7179869183E10)
        }
    }

    fn abs_max(a: f64, b: f64) -> f64 {
        if a.abs() > b.abs() { a } else { b }
    }
}

impl Serializable for LpVec3 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let i = buf.read_u8()?;
        if i == 0 {
            return Ok(Self(Vec3 {
                x: 0.,
                y: 0.,
                z: 0.,
            }));
        }

        let j = buf.read_u8()?;
        let l = buf.read_u32::<BigEndian>()?;
        let m: u64 = (l as u64) << 16 | (j as u64) << 8 | i as u64;
        let mut n: u64 = i as u64 & 3;
        if Self::has_fast_marker_bit(i as u32) {
            n |= (VarInt::read_from(buf)?.0 as u64 & 4294967295u64) << 2;
        }

        Ok(Self(Vec3 {
            x: (m >> 3) as f64 * n as f64,
            y: (m >> 18) as f64 * n as f64,
            z: (m >> 33) as f64 * n as f64,
        }))
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let d: f64 = Self::clamp_value(self.0.x);
        let e: f64 = Self::clamp_value(self.0.y);
        let f: f64 = Self::clamp_value(self.0.z);
        let g: f64 = Self::abs_max(d, Self::abs_max(e, f));
        if g < 3.051944088384301E-5 {
            buf.write_u8(0)?;
        } else {
            let l = g.ceil() as u64;
            let bl = (l & 3u64) != l;
            let m: u64 = if bl { l & 3u64 | 4u64 } else { l };
            let n = ((d / l as f64) as u64) << 3;
            let o = ((e / l as f64) as u64) << 18;
            let p = ((f / l as f64) as u64) << 33;
            let q: u64 = m | n | o | p;
            buf.write_u8(q as u8)?;
            buf.write_u8((q >> 8) as u8)?;
            buf.write_u32::<BigEndian>((q >> 16) as u32)?;
            if bl {
                VarInt((l >> 2) as i32).write_to(buf)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serializable)]
pub struct Clock {
    pub clock_id: VarInt,
    pub time: VarLong,
    pub fractional_time: f32,
    pub rate: f32,
}
