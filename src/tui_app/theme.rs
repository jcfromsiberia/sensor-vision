use ratatui::prelude::Stylize;

use strum_macros;
// bring the trait into scope
use strum::EnumProperty;

use ratatui::style::{Color, Styled};
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::model::sensor::{ValueType, ValueUnit};

pub static THEME_INDEX: AtomicUsize = AtomicUsize::new(0);

#[derive(strum_macros::EnumProperty)]
pub enum UIElement {
    /// Color indices according to https://en.wikipedia.org/wiki/ANSI_escape_code#8-bit
    /// `"dark_color, light_color"`
    #[strum(props(bg_colors = "232,255", fg_colors = "14,0"))]
    AppPad,

    #[strum(props(fg_colors = "14,0"))]
    InstructionsText,

    #[strum(props(fg_colors = "9,1"))]
    InstructionsActionText,

    #[strum(props(fg_colors = "9,9"))]
    NoSensors,

    #[strum(props(bg_colors = "21,39"))]
    SelectedSensorTab,

    #[strum(props(fg_colors = "9,9"))]
    NoMetrics,

    #[strum(props(fg_colors = "189,21"))]
    SensorName,

    #[strum(props(fg_colors = "117,57"))]
    SensorId,

    #[strum(props(fg_colors = "117,57"))]
    MetricId,

    #[strum(props(fg_colors = "189,21"))]
    MetricName,

    #[strum(props(fg_colors = "189,18"))]
    MetricValueType,

    #[strum(props(fg_colors = "189,18"))]
    MetricValueUnit,

    #[strum(props(fg_colors = "189,18"))]
    MetricValueAnnotation,

    #[strum(props(fg_colors = "252,233"))]
    MetricPropsBlock,

    #[strum(props(fg_colors = "33,202"))]
    MetricPropsBlockSelected,

    #[strum(props(fg_colors = "13,5"))]
    MetricNoData,

    #[strum(props(fg_colors = "4,2"))]
    LivedataLine,

    #[strum(props(fg_colors = "9,1"))]
    LivedataScatter,

    #[strum(props(bg_colors = "234,253"))]
    LivedataChart,

    #[strum(props(fg_colors = "7,15", bg_colors = "18,27"))]
    DialogPad,

    #[strum(props(fg_colors = "21,33", bg_colors = "18,27"))]
    OptionCard,

    #[strum(props(fg_colors = "129,202", bg_colors = "18,27"))]
    OptionCardSelected,

    #[strum(props(fg_colors = "15,0", bg_colors = "244,243"))]
    DialogButton,

    #[strum(props(fg_colors = "15,0", bg_colors = "45,214"))]
    DialogButtonFocused,

    #[strum(props(fg_colors = "15,15"))]
    DialogInstructionsText,

    #[strum(props(fg_colors = "9,220"))]
    DialogInstructionsActionText,

    #[strum(props(bg_colors = "238,250", fg_colors = "15,0"))]
    DialogTextInput,

    #[strum(props(bg_colors = "27,44", fg_colors = "15,0"))]
    DialogTextInputFocused,
}

impl UIElement {
    fn color_indices(&self) -> (Option<(Color, Color)>, Option<(Color, Color)>) {
        let mut bg_colors = None;
        let mut fg_colors = None;
        if let Some(colors) = self.get_str("bg_colors") {
            bg_colors = Self::parse_into_colors(colors);
        }
        if let Some(colors) = self.get_str("fg_colors") {
            fg_colors = Self::parse_into_colors(colors);
        }

        (bg_colors, fg_colors)
    }

    fn parse_into_colors(colors: &str) -> Option<(Color, Color)> {
        let split: Vec<&str> = colors.split(",").collect();
        if let [dark, light, ..] = split[..] {
            Some((
                Color::Indexed(dark.parse().unwrap()),
                Color::Indexed(light.parse().unwrap()),
            ))
        } else {
            None
        }
    }
}

pub trait ColorThemed<'a, T>: Stylize<'a, T> + Sized + Styled<Item = T> {
    fn themed(self, elem: UIElement) -> T {
        let mut style = self.style();
        let theme_idx = THEME_INDEX.load(Ordering::SeqCst);

        let (bg_colors, fg_colors) = elem.color_indices();

        if let Some((dark, light)) = bg_colors {
            match theme_idx {
                0 => style = style.bg(dark),
                1 => style = style.bg(light),
                _ => {}
            }
        }

        if let Some((dark, light)) = fg_colors {
            match theme_idx {
                0 => style = style.fg(dark),
                1 => style = style.fg(light),
                _ => {}
            }
        }
        self.set_style(style)
    }
}

impl<'a, T, U> ColorThemed<'a, T> for U where U: Stylize<'a, T> + Styled<Item = T> {}

pub trait Emojified {
    fn emojified(&self) -> String;
}

impl Emojified for ValueUnit {
    fn emojified(&self) -> String {
        let shortcode = match self {
            ValueUnit::Ampere
            | ValueUnit::Farad
            | ValueUnit::Ohm
            | ValueUnit::Volt => "zap",
            ValueUnit::Watt => "battery",
            ValueUnit::Bit => "keycap_ten",
            ValueUnit::Candela => "bulb",
            ValueUnit::Celsius => "thermometer",
            ValueUnit::Decibel => "bell",
            ValueUnit::Hertz => "signal_strength",
            ValueUnit::Joule => "battery",
            ValueUnit::Kilogram => "balance_scale",
            ValueUnit::Latitude | ValueUnit::Longitude => "world_map",
            ValueUnit::Meter => "straight_ruler",
            ValueUnit::MetersPerSecond => "bullettrain_side",
            ValueUnit::MetersPerSquareSecond => "rocket",
            ValueUnit::Mole => "test_tube",
            ValueUnit::Newton => "apple",
            ValueUnit::Pascal => "tornado",
            ValueUnit::Percent => "100",
            ValueUnit::Radian | ValueUnit::SquareMetre => "triangular_ruler",
            ValueUnit::Second => "watch",
        };
        format!(
            "{} {:?}",
            emojis::get_by_shortcode(shortcode).expect(&format!("Missing shortcode {shortcode}")),
            self
        )
    }
}

impl Emojified for ValueType {
    fn emojified(&self) -> String {
        let shortcode = match self {
            ValueType::Boolean => "keycap_ten",
            ValueType::Integer => "1234",
            ValueType::Double => "heavy_division_sign",
            ValueType::String => "pencil",
        };
        format!(
            "{} {:?}",
            emojis::get_by_shortcode(shortcode).unwrap(),
            self
        )
    }
}