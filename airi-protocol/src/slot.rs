use crate::{
    IdOrX, IdSet, Identifier, Lengthable, Position, PrefixedArray, ReadingError, Serializable,
    TextComponent, UUID, VarInt, WritingError, nbt, packet::ProfileProperty,
};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

#[derive(Debug, Serializable)]
pub struct HashedStack {
    pub item_id: VarInt,
    pub item_count: VarInt,
    pub components_to_add: PrefixedArray<HashedComponent>,
    pub components_to_remove: PrefixedArray<VarInt>,
}

#[derive(Debug, Serializable)]
pub struct HashedComponent {
    pub component_type: VarInt,
    pub component_data_hash: u32,
}

#[derive(Debug)]
pub struct Slot {
    pub item_count: VarInt,
    pub item: Option<Item>,
}

pub type HashedSlot = Option<HashedStack>;

impl Serializable for Slot {
    fn read_from<R: std::io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let item_count = VarInt::read_from(buf)?;
        let item = (item_count.0 > 0)
            .then(|| Serializable::read_from(buf))
            .transpose()?;

        Ok(Slot { item_count, item })
    }
    fn write_to<W: std::io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        self.item_count.write_to(buf)?;
        if let Some(val) = &self.item {
            val.write_to(buf)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Item {
    pub item_id: VarInt,
    pub components_to_add: Vec<Component>,
    pub components_to_remove: Vec<VarInt>,
}

impl Serializable for Item {
    fn read_from<R: std::io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let item_id = VarInt::read_from(buf)?;
        let components_to_add_len = VarInt::read_from(buf)?;
        let components_to_remove_len = VarInt::read_from(buf)?;

        let mut components_to_add = Vec::new();
        let mut components_to_remove = Vec::new();

        for _ in 0..components_to_add_len.0 {
            components_to_add.push(Serializable::read_from(buf)?);
        }

        for _ in 0..components_to_remove_len.0 {
            components_to_remove.push(Serializable::read_from(buf)?);
        }

        Ok(Item {
            item_id,
            components_to_add,
            components_to_remove,
        })
    }
    fn write_to<W: std::io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        self.item_id.write_to(buf)?;
        VarInt::from_len(self.components_to_add.len()).write_to(buf)?;
        VarInt::from_len(self.components_to_remove.len()).write_to(buf)?;
        for c in &self.components_to_add {
            c.write_to(buf)?;
        }
        for c in &self.components_to_remove {
            c.write_to(buf)?;
        }
        Ok(())
    }
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum Component {
    CustomData {
        data: nbt::Tag,
    },
    MaxStackSize {
        max_stack_size: VarInt,
    },
    MaxDamage {
        max_damage: VarInt,
    },
    Damage {
        damage: VarInt,
    },
    Unbreakable,
    CustomName {
        name: TextComponent,
    },
    ItemName {
        name: TextComponent,
    },
    ItemModel {
        model: Identifier,
    },
    Lore {
        lines: PrefixedArray<TextComponent>,
    },
    Rarity {
        rarity: Rarity,
    },
    Enchantments {
        enchantments: PrefixedArray<Enchantment>,
    },
    CanPlaceOn {
        block_predicates: PrefixedArray<BlockPredicate>,
    },
    CanBreak {
        block_predicates: PrefixedArray<BlockPredicate>,
    },
    AttributeModifiers(PrefixedArray<AttributeModifier>),
    CustomModelData {
        floats: PrefixedArray<f32>,
        flags: PrefixedArray<bool>,
        strings: PrefixedArray<String>,
        colors: PrefixedArray<i32>,
    },
    TooltipDisplay {
        hide_tooltip: bool,
        hidden_components: PrefixedArray<VarInt>,
    },
    RepairCost {
        repair_cost: VarInt,
    },
    CreativeSlotLock,
    EnchantmentGlintOverride {
        has_glint: bool,
    },
    IntangibleProjectile {
        empty: nbt::Tag,
    },
    Food {
        nutrition: VarInt,
        saturation_modifier: f32,
        can_always_eat: bool,
    },
    Consumable {
        consume_seconds: f32,
        animation: ConsumeAnimation,
        sound: IdOrX<SoundEvent>,
        has_consume_particles: bool,
        effects: PrefixedArray<ConsumeEffect>,
    },
    UseRemainder {
        remainder: Slot,
    },
    UseCooldown {
        seconds: f32,
        cooldown_group: Option<Identifier>,
    },
    DamageResistant {
        types: Identifier,
    },
    Tool {
        rules: PrefixedArray<ToolRule>,
        default_mining_speed: f32,
        damage_per_block: VarInt,
    },
    Weapon {
        damage_per_attack: VarInt,
        disable_blocking_for: f32,
    },
    Enchantable {
        value: VarInt,
    },
    Equippable {
        slot: EquippableSlot,
        equip_sound: IdOrX<SoundEvent>,
        model: Option<Identifier>,
        camera_overlay: Option<Identifier>,
        allowed_entities: Option<IdSet>,
        dispensable: bool,
        swappable: bool,
        damage_on_hurt: bool,
    },
    Repairable {
        items: IdSet,
    },
    Glider,
    TooltipStyle {
        style: Identifier,
    },
    DeathProtection {
        effects: PrefixedArray<ConsumeEffect>,
    },
    BlocksAttacks {
        block_delay_seconds: f32,
        disable_cooldown_scale: f32,
        damage_reductions: PrefixedArray<DamageReduction>,
        item_damage_threshold: f32,
        item_damage_base: f32,
        item_damage_factor: f32,
        bypassed_by: Option<Identifier>,
        block_sound: Option<IdOrX<SoundEvent>>,
        disable_sound: Option<IdOrX<SoundEvent>>,
    },
    StoredEnchantments {
        enchantments: PrefixedArray<Enchantment>,
    },
    DyedColor {
        color: ColorI32,
    },
    MapColor {
        color: ColorI32,
    },
    MapId {
        id: VarInt,
    },
    MapDecorations {
        data: nbt::Tag,
    },
    MapPostProcessing {
        ty: MapPostProcessingType,
    },
    ChargedProjectiles {
        projectiles: PrefixedArray<Slot>,
    },
    BundleContents {
        items: PrefixedArray<Slot>,
    },
    PotionContents {
        potion_id: Option<VarInt>,
        custom_color: Option<ColorI32>,
    },
    PotionDurationScale {
        effect_multiplier: f32,
    },
    SuspiciousStewEffects {
        effects: PrefixedArray<SuspiciousStewEffect>,
    },
    WritableBookContent {
        raw_content: String,
        filtered_content: Option<String>,
    },
    WrittenBookContent {
        raw_title: String,
        filtered_title: Option<String>,
        author: String,
        generation: VarInt,
        pages: PrefixedArray<BookPage>,
        resolved: bool,
    },
    Trim {
        trim_material: IdOrX<TrimMaterial>,
        trim_pattern: IdOrX<TrimPattern>,
    },
    DebugStickState {
        data: nbt::Tag,
    },
    EntityData {
        data: nbt::Tag,
    },
    BucketEntityData {
        data: nbt::Tag,
    },
    BlockEntityData {
        data: nbt::Tag,
    },
    Instrument(IdOrX<Instrument>),
    ProvidesTrimMaterial(ProvidesTrimMaterialMode),
    OminousBottleAmplifier {
        amplifier: VarInt,
    },
    JukeboxPlayable(JukeboxPlayable),
    ProvidesBannerPatterns {
        key: Identifier,
    },
    Recipes {
        data: nbt::Tag,
    },
    LodestoneTracker {
        has_global_position: bool,
        dimension: Identifier,
        position: Position,
        tracked: bool,
    },
    FireworkExplosion {
        explosion: FireworkExplosion,
    },
    Fireworks {
        flight_duration: VarInt,
        explosions: PrefixedArray<FireworkExplosion>,
    },
    Profile {
        name: Option<String>,
        unique_id: Option<UUID>,
        properties: PrefixedArray<ProfileProperty>,
    },
    NoteBlockSound {
        sound: Identifier,
    },
    BannerPatterns {
        layers: PrefixedArray<BannerLayer>,
    },
    BaseColor(DyeColor),
    PotDecorations(PrefixedArray<VarInt>),
    Container {
        items: PrefixedArray<Slot>,
    },
    BlockState {
        properties: PrefixedArray<BlockStateProperty>,
    },
    Bees(PrefixedArray<Bee>),
    Lock {
        key: nbt::Tag,
    },
    ContainerLoot {
        data: nbt::Tag,
    },
    BreakSound {
        sound_event: IdOrX<SoundEvent>,
    },
    VillagerVariant(VarInt),
    WolfVariant(VarInt),
    WolfSoundVariant(VarInt),
    WolfCollar {
        color: DyeColor,
    },
    FoxVariant(FoxVariant),
    SalmonSize {
        ty: SalmonSize,
    },
    ParrotVariant(VarInt),
    TropicalFishPattern(TropicalFishPattern),
    TropicalFishBaseColor(DyeColor),
    TropicalFishPatternColor(DyeColor),
    MooshroomVariant(MooshroomVariant),
    RabbitVariant(RabbitVariant),
    PigVariant(VarInt),
    CowVariant(VarInt),
    ChickenVariant(ChickenVariant),
    FrogVariant(VarInt),
    HorseVariant(HorseVariant),
    PaintingVariant(PaintingVariant),
    LlamaVariant(LlamaVariant),
    AxolotlVariant(AxolotlVariant),
    CatVariant(VarInt),
    CatCollar {
        color: DyeColor,
    },
    SheepColor(DyeColor),
    ShulkerColor(DyeColor),
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
}

#[derive(Debug, Serializable)]
pub struct Enchantment {
    pub type_id: VarInt,
    pub level: VarInt,
}

#[derive(Debug, Serializable)]
pub struct BlockPredicate {
    pub blocks: Option<IdSet>,
    pub properties: Option<PrefixedArray<Property>>,
    pub nbt: Option<nbt::Tag>,
    pub data_components: PrefixedArray<ExactDataComponentMatcher>,
    pub partial_data_component_predicates: PrefixedArray<PartialDataComponentMatcher>,
}

#[derive(Debug, Serializable)]
pub struct Property {
    pub name: String,
    pub match_type: PropertyMatch,
}

#[derive(Debug, Serializable)]
#[enum_info(bool)]
pub enum PropertyMatch {
    RangedMatch { min: String, max: String },
    ExactMatch(String),
}

#[derive(Debug, Serializable)]
pub struct ExactDataComponentMatcher(pub Component);

#[derive(Debug, Serializable)]
pub struct PartialDataComponentMatcher {
    pub ty: PartialDataComponentMatcherType,
    pub predicate: nbt::Tag,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum PartialDataComponentMatcherType {
    Damage,
    Enchantments,
    StoredEnchantments,
    PotionContents,
    CustomData,
    Container,
    BundleContents,
    FireworkExplosion,
    Fireworks,
    WritableBookContent,
    WrittenBookContent,
    AttributeModifiers,
    Trim,
    JukeboxPlayable,
}

#[derive(Debug, Serializable)]
pub struct AttributeModifier {
    pub attribute_id: VarInt,
    pub modifier_id: Identifier,
    pub value: f64,
    pub operation: AttributeOperation,
    pub slot: AttributeModifierSlot,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum AttributeOperation {
    Add,
    MultiplyBase,
    MultiplyTotal,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum AttributeModifierSlot {
    Any,
    MainHand,
    OffHand,
    Hand,
    Feet,
    Legs,
    Chest,
    Head,
    Armor,
    Body,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ConsumeAnimation {
    None,
    Eat,
    Drink,
    Block,
    Bow,
    Spear,
    Crossbow,
    Spyglass,
    TootHorn,
    Brush,
}

#[derive(Debug, Serializable)]
pub struct SoundEvent {
    pub sound_name: Identifier,
    pub fixed_range: Option<f32>,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum ConsumeEffect {
    ApplyEffects {
        effects: PrefixedArray<PotionEffect>,
        probability: f32,
    },
    RemoveEffects {
        effects: IdSet,
    },
    ClearAllEffects,
    TeleportRandomly {
        diameter: f32,
    },
    PlaySound {
        sound: SoundEvent,
    },
}

#[derive(Debug, Serializable)]
pub struct PotionEffect {
    pub type_id: VarInt,
    pub details: PotionEffectDetail,
}

#[derive(Debug, Serializable)]
pub struct PotionEffectDetail {
    pub amplifier: VarInt,
    /// -1 for infinite
    pub duration: VarInt,
    pub ambient: bool,
    pub show_particles: bool,
    pub show_icon: bool,
    pub hidden_effect: Option<Box<PotionEffectDetail>>,
}

#[derive(Debug, Serializable)]
pub struct ToolRule {
    pub blocks: IdSet,
    pub speed: Option<f32>,
    pub correct_drop_for_blocks: Option<bool>,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum EquippableSlot {
    Mainhand,
    Feet,
    Legs,
    Chest,
    Head,
    Offhand,
    Body,
}

#[derive(Debug, Serializable)]
pub struct DamageReduction {
    pub horizontal_blocking_angle: f32,
    pub ty: Option<IdSet>,
    pub base: f32,
    pub factor: f32,
}

/// Color as 0xRRGGBB, top bits are ignored
#[derive(Debug)]
pub struct ColorI32 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Serializable for ColorI32 {
    fn read_from<R: std::io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let int = buf.read_u32::<BigEndian>()?;
        let r = (int >> 16) as u8;
        let g = (int >> 8) as u8;
        let b = int as u8;
        Ok(ColorI32 { r, g, b })
    }
    fn write_to<W: std::io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut int = self.b as u32;
        int |= (self.g as u32) << 8;
        int |= (self.r as u32) << 16;
        buf.write_u32::<BigEndian>(int)?;
        Ok(())
    }
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum MapPostProcessingType {
    Lock,
    Scale,
}

#[derive(Debug, Serializable)]
pub struct SuspiciousStewEffect {
    pub type_id: VarInt,
    pub duration: VarInt,
}

#[derive(Debug, Serializable)]
pub struct BookPage {
    pub raw_content: String,
    pub filtered_content: Option<String>,
}

#[derive(Debug, Serializable)]
pub struct TrimMaterial {
    pub suffix: String,
    pub overrides: PrefixedArray<TrimMaterialOverrides>,
    pub description: TextComponent,
}

#[derive(Debug, Serializable)]
pub struct TrimMaterialOverrides {
    pub armor_material_type: Identifier,
    pub overriden_asset_name: String,
}

#[derive(Debug, Serializable)]
pub struct TrimPattern {
    pub asset_name: String,
    pub template_item: VarInt,
    pub description: TextComponent,
    pub decal: bool,
}

#[derive(Debug, Serializable)]
pub struct Instrument {
    pub sound_event: IdOrX<SoundEvent>,
    pub sound_range: f32,
    pub range: f32,
    pub description: TextComponent,
}

#[derive(Debug, Serializable)]
pub struct JukeboxSong {
    pub sound_event: IdOrX<SoundEvent>,
    pub description: TextComponent,
    pub duration: f32,
    pub output: VarInt,
}

#[derive(Debug, Serializable)]
#[enum_info(i8)]
pub enum ProvidesTrimMaterialMode {
    Identifier(Identifier),
    IdOr(IdOrX<TrimMaterial>),
}

#[derive(Debug, Serializable)]
#[enum_info(i8)]
pub enum JukeboxPlayable {
    Identifier(Identifier),
    IdOr(IdOrX<JukeboxSong>),
}

#[derive(Debug, Serializable)]
pub struct FireworkExplosion {
    pub shape: FireworkExplosionShape,
    pub colors: PrefixedArray<ColorI32>,
    pub fade_colors: PrefixedArray<ColorI32>,
    pub has_trail: bool,
    pub has_twinkle: bool,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum FireworkExplosionShape {
    SmallBall,
    LargeBall,
    Star,
    Creeper,
    Burst,
}

#[derive(Debug, Serializable)]
pub struct BannerLayer {
    pub pattern_type: IdOrX<BannerLayerData>,
    pub color: DyeColor,
}

#[derive(Debug, Serializable)]
pub struct BannerLayerData {
    pub asset_id: Identifier,
    pub translation_key: String,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum DyeColor {
    White,
    Orange,
    Magenta,
    LightBlue,
    Yellow,
    Lime,
    Pink,
    Gray,
    LightGray,
    Cyan,
    Purple,
    Blue,
    Brown,
    Green,
    Red,
    Black,
}

#[derive(Debug, Serializable)]
pub struct BlockStateProperty {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serializable)]
pub struct Bee {
    pub entity_data: nbt::Tag,
    pub ticks_in_hive: VarInt,
    pub min_ticks_in_hive: VarInt,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum FoxVariant {
    Red,
    Snow,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum SalmonSize {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum TropicalFishPattern {
    Kob,
    Sunstreak,
    Snooper,
    Dasher,
    Brinely,
    Spotty,
    Flopper,
    Stripey,
    Glitter,
    Blockfish,
    Betty,
    Clayfish,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum MooshroomVariant {
    Red,
    Brown,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum RabbitVariant {
    Brown,
    White,
    Black,
    WhiteSplotched,
    Gold,
    Salt,
    Evil,
}

#[derive(Debug, Serializable)]
#[enum_info(i8)]
pub enum ChickenVariant {
    Identifier(Identifier),
    Id(VarInt),
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum HorseVariant {
    White,
    Creamy,
    Chestnut,
    Brown,
    Black,
    Gray,
    DarkBrown,
}

#[derive(Debug, Serializable)]
pub struct PaintingVariant {
    pub width: i32,
    pub height: i32,
    pub asset_id: Identifier,
    pub title: Option<TextComponent>,
    pub author: Option<TextComponent>,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum LlamaVariant {
    Creamy,
    White,
    Brown,
    Gray,
}

#[derive(Debug, Serializable)]
#[enum_info(VarInt)]
pub enum AxolotlVariant {
    Lucy,
    Wild,
    Gold,
    Cyan,
    Blue,
}
