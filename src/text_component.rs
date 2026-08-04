// UNFINISHED, when I use serde for nbt I will finish this


use crate::{Identifier, UUID, nbt, packet::ColorARGBI32};

#[derive(Debug)]
struct TextComponent(pub TextComponentBase);

#[derive(Debug)]
struct TextComponentBase {
    pub content: TextContent,

    pub style: Style,

    pub extra: Vec<TextComponentBase>,
}

// impl Serializable for TextComponentBase {
//     fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, crate::ReadingError> {
//         let nbt = nbt::Tag::read_from(buf)?;
//         match nbt {
//             nbt::Tag::String(str) => Ok(TextComponentBase {
//                 content: TextContent::Text(str),
//                 style: Default::default(),
//                 extra: vec![],
//             }),
//             _ => return Err(ReadingError::Message(format!(""))),
//         }
//     }
//     fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), crate::WritingError> {

//     }
// }

#[derive(Debug, Default)]
pub struct Style {
    pub color: Option<Color>,
    pub font: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underli: Option<bool>,
    pub striket: Option<bool>,
    pub obfuscated: Option<bool>,
    pub shadow_color: Option<ColorARGBI32>,
    pub insertion: Option<String>,
    pub click_event: Option<ClickEvent>,
    pub hover_event: Option<HoverEvent>,
}

#[derive(Debug)]
enum TextContent {
    Text(String),
    TranslatedText(TranslatedTextInfo),
    ScoreboardValue {
        name: String,
        objective: String,
    },
    Selector {
        selector: String,
        seperator: Option<String>,
    },
    Keybind {
        identifier: String,
    },
    Nbt {
        content: NbtContent,
        seperator: Option<String>,
    },
}

#[derive(Debug)]
pub enum TranslatedTextInfo {
    // If there is no "translate", everything else is ignored
    Empty,
    Text {
        translate: Identifier,
        fallback: Option<String>,
        with: Vec<TextComponentBase>,
    },
}

#[derive(Debug)]
pub enum NbtContent {
    NoSource,
    WithSource {
        source: String,
        nbt: nbt::Tag,
        interpret: bool,
        block: String,
        entity: String,
        storage: String,
    },
}

#[derive(Default, Debug)]
pub enum Color {
    /// The default color for the text will be used, which varies by context
    /// (in some cases, it's white; in others, it's black; in still others, it
    /// is a shade of gray that isn't normally used on text).
    #[default]
    Reset,
    /// RGB Color
    Rgb(RGBColor),
    /// One of the 16 named Minecraft colors
    Named(NamedColor),
}

#[derive(Debug)]
pub struct RGBColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug)]
pub enum NamedColor {
    Black = 0,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

#[derive(Debug)]
pub enum ClickEvent {
    OpenUrl(String),
    OpenFile(String),
    /// Doesnt need to be prefixed by a /
    RunCommand(String),
    SuggestCommand(String),
    ChangePage(i16),
    CopyToClipboard(String),
    ShowDialog(DialogType),
    Custom {
        identifier: String,
        payload: Option<String>,
    },
}

#[derive(Debug)]
enum DialogType {
    Identifier(String),
    Custom(Box<Dialog>),
}

#[derive(Debug)]
pub enum HoverEvent {
    ShowText(Box<TextComponent>),
    ShowItem(nbt::Tag),
    ShowEntity {
        name: Option<String>,
        ident: String,
        uuid: UUID,
    },
}

#[derive(Debug)]
pub struct Dialog {
    pub title: TextComponent,
    pub external_title: Option<TextComponent>,
    pub body: DialogBodyType,
}

#[derive(Debug)]
pub enum DialogBodyType {
    One(DialogBody),
    Many(Vec<DialogBody>),
}

#[derive(Debug)]
pub enum DialogBody {
    PlainMessage {
        contents: Box<TextComponent>,
        width: i32,
    },
    // im too tired for this shi
    Item(nbt::Tag),
}
