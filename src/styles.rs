//! Material 3 (Material You) styling system for the UU accelerator GUI.
//!
//! Implements the M3 dark color scheme (blue seed) component shapes:
//! filled / tonal / outlined / text buttons, filled cards, outlined text
//! fields, dropdowns, segmented tabs, snackbars (toasts) and thin
//! scrollbars — with proper state layers (hover = 8% overlay,
//! pressed = 10%, disabled = 38% alpha).

use iced::border::Radius;
use iced::widget::{button, container, pick_list, scrollable, text_input};
use iced::{border, Background, Border, Color, Shadow, Theme};

use button::Status;

/// Top-rounded tab shape (bottom edge straight, connecting with content).
fn tab_shape() -> Radius {
    Radius {
        top_left: 10.0,
        top_right: 10.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    }
}
// ---------------------------------------------------------------------------
// Palette — M3 dark scheme, blue seed (Google blue dynamic-color values)
// ---------------------------------------------------------------------------

pub struct C;

/// Faded variant of a color (alpha stack for toast animations).
pub fn faded(c: Color, a: f32) -> Color {
    Color {
        a: c.a * a,
        ..c
    }
}

impl C {
    pub const PRIMARY: Color = hex(0xA8C7FA);
    pub const ON_PRIMARY: Color = hex(0x062E6F);
    pub const PRIMARY_CONTAINER: Color = hex(0x0842A0);
    pub const ON_PRIMARY_CONTAINER: Color = hex(0xD3E3FD);

    pub const SECONDARY_CONTAINER: Color = hex(0x004A77);
    pub const ON_SECONDARY_CONTAINER: Color = hex(0xC2E7FF);

    pub const ERROR: Color = hex(0xF2B8B5);
    #[allow(dead_code)]
    pub const ON_ERROR: Color = hex(0x601410);
    pub const ERROR_CONTAINER: Color = hex(0x8C1D18);
    pub const ON_ERROR_CONTAINER: Color = hex(0xF9DEDC);

    pub const GREEN: Color = hex(0x6DD58C);
    pub const ON_GREEN: Color = hex(0x072711);
    pub const GREEN_CONTAINER: Color = hex(0x0F5223);
    pub const ON_GREEN_CONTAINER: Color = hex(0xC4EED0);

    // M3 amber used as the theme's `warning` accent (0.14 palette field).
    pub const WARN: Color = hex(0xFFB95C);

    pub const SURFACE: Color = hex(0x111318);
    pub const SURFACE_LOW: Color = hex(0x191C20);
    pub const SURFACE_CONTAINER: Color = hex(0x1D2024);
    pub const SURFACE_HIGH: Color = hex(0x282A2F);
    pub const SURFACE_HIGHEST: Color = hex(0x33343A);

    pub const ON_SURFACE: Color = hex(0xE2E2E9);
    pub const ON_SURFACE_VARIANT: Color = hex(0xC4C6CF);
    pub const OUTLINE: Color = hex(0x8D9199);
    pub const OUTLINE_VARIANT: Color = hex(0x43474E);

    pub const INVERSE_SURFACE: Color = hex(0xE2E2E9);
    pub const INVERSE_ON_SURFACE: Color = hex(0x2E3036);

    /// Hover border color for inputs.
    pub const INVERSE: Color = hex(0xE2E2E9);
}

pub const fn hex(rgb: u32) -> Color {
    Color::from_rgb(
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    )
}

/// The application theme (M3 dark, blue seed).
pub fn theme() -> Theme {
    // 0.15: `Theme::custom` takes a `Seed` (undistinguished colors); the
    // derived `Palette`/`Swatch` shades are generated from it (Oklch).
    Theme::custom(
        String::from("m3-blue-dark"),
        iced::theme::palette::Seed {
            background: C::SURFACE,
            text: C::ON_SURFACE,
            primary: C::PRIMARY,
            success: C::GREEN,
            danger: C::ERROR,
            warning: C::WARN,
        },
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// M3 state layer: blend `overlay` color into `base` with the given opacity.
fn state_layer(base: Color, overlay: Color, t: f32) -> Color {
    Color {
        r: base.r * (1.0 - t) + overlay.r * t,
        g: base.g * (1.0 - t) + overlay.g * t,
        b: base.b * (1.0 - t) + overlay.b * t,
        a: base.a * (1.0 - t) + overlay.a * t,
    }
}

fn dis_fg() -> Color {
    Color {
        a: 0.38,
        ..C::ON_SURFACE
    }
}

fn pill() -> Border {
    Border {
    color: Color::TRANSPARENT,
        width: 0.0,
        radius: Radius::from(999.0),
    }
}

fn rounded(r: f32) -> Border {
    Border {
    color: Color::TRANSPARENT,
        width: 0.0,
        radius: Radius::from(r),
    }
}

fn flat() -> Shadow {
    Shadow {
    color: Color::TRANSPARENT,
        offset: iced::Vector::ZERO,
        blur_radius: 0.0,
    }
}

/// Build a button style with M3 defaults (pill shape, flat shadow).
fn bs(background: Option<Color>, text_color: Color, border: Border) -> button::Style {
    button::Style {
        snap: false,
        background: background.map(Background::Color),
        text_color,
        border,
        shadow: flat(),
    }
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// M3 filled button (pill, primary).
pub fn filled(_theme: &Theme, status: Status) -> button::Style {
    match status {
        Status::Disabled => bs(
            Some(state_layer(C::SURFACE, C::ON_SURFACE, 0.12)),
            dis_fg(),
            pill(),
        ),
        Status::Hovered => bs(
            Some(state_layer(C::PRIMARY, Color::WHITE, 0.08)),
            C::ON_PRIMARY,
            pill(),
        ),
        Status::Pressed => bs(
            Some(state_layer(C::PRIMARY, Color::WHITE, 0.10)),
            C::ON_PRIMARY,
            pill(),
        ),
        _ => bs(Some(C::PRIMARY), C::ON_PRIMARY, pill()),
    }
}

/// M3 filled tonal button (secondary container).
pub fn tonal(_theme: &Theme, status: Status) -> button::Style {
    match status {
        Status::Disabled => bs(
            Some(state_layer(C::SURFACE, C::ON_SURFACE, 0.12)),
            dis_fg(),
            pill(),
        ),
        Status::Hovered => bs(
            Some(state_layer(C::SECONDARY_CONTAINER, Color::WHITE, 0.08)),
            C::ON_SECONDARY_CONTAINER,
            pill(),
        ),
        Status::Pressed => bs(
            Some(state_layer(C::SECONDARY_CONTAINER, Color::WHITE, 0.10)),
            C::ON_SECONDARY_CONTAINER,
            pill(),
        ),
        _ => bs(
            Some(C::SECONDARY_CONTAINER),
            C::ON_SECONDARY_CONTAINER,
            pill(),
        ),
    }
}

/// M3 outlined button.
pub fn outlined(_theme: &Theme, status: Status) -> button::Style {
    let base_border = Border {
color: C::OUTLINE,
        width: 1.0,
        radius: pill().radius,
    };
    match status {
        Status::Disabled => bs(
            None,
            dis_fg(),
            Border {
                color: Color {
                    a: 0.12,
                    ..C::ON_SURFACE
                },
                width: 1.0,
                radius: pill().radius,
            },
        ),
        Status::Hovered => bs(
            Some(state_layer(C::SURFACE, C::PRIMARY, 0.08)),
            C::PRIMARY,
            base_border,
        ),
        Status::Pressed => bs(
            Some(state_layer(C::SURFACE, C::PRIMARY, 0.10)),
            C::PRIMARY,
            base_border,
        ),
        _ => bs(None, C::PRIMARY, base_border),
    }
}

/// M3 text button (no background, primary label).
pub fn text_btn(_theme: &Theme, status: Status) -> button::Style {
    let fg = match status {
        Status::Disabled => dis_fg(),
        _ => C::PRIMARY,
    };
    let bg = match status {
        Status::Hovered => Some(state_layer(C::SURFACE, C::ON_SURFACE, 0.08)),
        Status::Pressed => Some(state_layer(C::SURFACE, C::ON_SURFACE, 0.10)),
        _ => None,
    };
    bs(bg, fg, pill())
}

/// Filled error button.
#[allow(dead_code)]
pub fn error_filled(_theme: &Theme, status: Status) -> button::Style {
    match status {
        Status::Hovered => bs(
            Some(state_layer(C::ERROR, Color::WHITE, 0.08)),
            C::ON_ERROR,
            pill(),
        ),
        Status::Pressed => bs(
            Some(state_layer(C::ERROR, Color::WHITE, 0.10)),
            C::ON_ERROR,
            pill(),
        ),
        _ => bs(Some(C::ERROR), C::ON_ERROR, pill()),
    }
}

/// Filled tonal error button.
pub fn error_tonal(_theme: &Theme, status: Status) -> button::Style {
    match status {
        Status::Hovered => bs(
            Some(state_layer(C::ERROR_CONTAINER, Color::WHITE, 0.08)),
            C::ON_ERROR_CONTAINER,
            pill(),
        ),
        Status::Pressed => bs(
            Some(state_layer(C::ERROR_CONTAINER, Color::WHITE, 0.10)),
            C::ON_ERROR_CONTAINER,
            pill(),
        ),
        _ => bs(Some(C::ERROR_CONTAINER), C::ON_ERROR_CONTAINER, pill()),
    }
}

/// Filled success button.
pub fn success_filled(_theme: &Theme, status: Status) -> button::Style {
    match status {
        Status::Hovered => bs(
            Some(state_layer(C::GREEN, Color::BLACK, 0.06)),
            C::ON_GREEN,
            pill(),
        ),
        Status::Pressed => bs(
            Some(state_layer(C::GREEN, Color::BLACK, 0.10)),
            C::ON_GREEN,
            pill(),
        ),
        _ => bs(Some(C::GREEN), C::ON_GREEN, pill()),
    }
}

/// Filled tonal success button.
#[allow(dead_code)]
pub fn success_tonal(_theme: &Theme, status: Status) -> button::Style {
    match status {
        Status::Hovered => bs(
            Some(state_layer(C::GREEN_CONTAINER, Color::WHITE, 0.08)),
            C::ON_GREEN_CONTAINER,
            pill(),
        ),
        Status::Pressed => bs(
            Some(state_layer(C::GREEN_CONTAINER, Color::WHITE, 0.10)),
            C::ON_GREEN_CONTAINER,
            pill(),
        ),
        _ => bs(Some(C::GREEN_CONTAINER), C::ON_GREEN_CONTAINER, pill()),
    }
}

/// M3 tonal icon button: circular, secondary-container background.
/// (刷新设备 uses this — blue circle, light glyph.)
pub fn icon_tonal(_theme: &Theme, status: Status) -> button::Style {
    let (bg, fg) = match status {
        Status::Hovered => (
            state_layer(C::SECONDARY_CONTAINER, Color::WHITE, 0.08),
            C::ON_SECONDARY_CONTAINER,
        ),
        Status::Pressed => (
            state_layer(C::SECONDARY_CONTAINER, Color::WHITE, 0.10),
            C::ON_SECONDARY_CONTAINER,
        ),
        _ => (C::SECONDARY_CONTAINER, C::ON_SECONDARY_CONTAINER),
    };
    button::Style {
            snap: false,
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::from(999.0),
        },
        shadow: flat(),
    }
}

/// M3 neutral icon button: circular, thin outline ring.
/// (退出登录 uses this.)
pub fn icon_outlined(_theme: &Theme, status: Status) -> button::Style {
    let (bg, ring) = match status {
        Status::Hovered | Status::Pressed => (
            Some(Background::Color(state_layer(
                C::SURFACE,
                C::ON_SURFACE,
                0.08,
            ))),
            C::ON_SURFACE_VARIANT,
        ),
        _ => (None, C::OUTLINE),
    };
    button::Style {
            snap: false,
        background: bg,
        text_color: C::PRIMARY,
        border: Border {
            color: ring,
            width: 1.0,
            radius: Radius::from(999.0),
        },
        shadow: flat(),
    }
}

/// Frameless-window control: small circular hover chip; close turns red.
pub fn win_ctl(_theme: &Theme, status: Status) -> button::Style {
    let bg = match status {
        Status::Hovered => Some(Background::Color(state_layer(
            C::SURFACE,
            C::ON_SURFACE,
            0.08,
        ))),
        Status::Pressed => Some(Background::Color(state_layer(
            C::SURFACE,
            C::ON_SURFACE,
            0.12,
        ))),
        _ => None,
    };
    button::Style {
            snap: false,
        background: bg,
        text_color: C::ON_SURFACE_VARIANT,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::from(999.0),
        },
        shadow: flat(),
    }
}

pub fn win_close(_theme: &Theme, status: Status) -> button::Style {
    let (bg, fg) = match status {
        Status::Hovered | Status::Pressed => (
            Background::Color(Color {
                a: 0.8,
                ..C::ERROR_CONTAINER
            }),
            C::ON_ERROR_CONTAINER,
        ),
        _ => (Background::Color(Color::TRANSPARENT), C::ON_SURFACE_VARIANT),
    };
    button::Style {
            snap: false,
        background: Some(bg),
        text_color: fg,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::from(999.0),
        },
        shadow: flat(),
    }
}

/// Active tab of the explorer-style strip: painted with the card surface
/// color and connected to the card's top edge (its bottom corners square).
pub fn tab_active(_theme: &Theme, status: Status) -> button::Style {
    let bg = match status {
        Status::Hovered => Some(Background::Color(state_layer(
            C::SURFACE_CONTAINER,
            C::ON_SURFACE,
            0.06,
        ))),
        Status::Pressed => Some(Background::Color(state_layer(
            C::SURFACE_CONTAINER,
            C::ON_SURFACE,
            0.10,
        ))),
        _ => Some(Background::Color(C::SURFACE_CONTAINER)),
    };
    button::Style {
            snap: false,
        background: bg,
        text_color: C::ON_SURFACE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: tab_shape(),
        },
        shadow: flat(),
    }
}

/// Inactive tab: transparent (blends into the strip) — only the label
/// floats, with a subtle hover layer; the active tab is the only chip.
pub fn tab_inactive(_theme: &Theme, status: Status) -> button::Style {
    let bg = match status {
        Status::Hovered => Some(Background::Color(state_layer(
            C::SURFACE,
            C::ON_SURFACE,
            0.06,
        ))),
        Status::Pressed => Some(Background::Color(state_layer(
            C::SURFACE,
            C::ON_SURFACE,
            0.10,
        ))),
        _ => None,
    };
    button::Style {
            snap: false,
        background: bg,
        text_color: C::ON_SURFACE_VARIANT,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: tab_shape(),
        },
        shadow: flat(),
    }
}

// ---------------------------------------------------------------------------
// Device cards (clickable M3 cards)
// ---------------------------------------------------------------------------

/// Resting device card.
pub fn device_card(_theme: &Theme, status: Status) -> button::Style {
    let (bg, border) = match status {
        Status::Hovered | Status::Pressed => (
            state_layer(C::SURFACE_HIGH, C::ON_SURFACE, 0.08),
            C::OUTLINE,
        ),
        _ => (C::SURFACE_HIGH, C::OUTLINE_VARIANT),
    };
    button::Style {
            snap: false,
        background: Some(Background::Color(bg)),
        text_color: C::ON_SURFACE,
        border: Border {
            color: border,
            width: 1.0,
            radius: Radius::from(12.0),
        },
        shadow: flat(),
    }
}

/// Selected device card (primary container, 2px primary border).
pub fn device_card_selected(_theme: &Theme, status: Status) -> button::Style {
    let bg = match status {
        Status::Hovered | Status::Pressed => {
            state_layer(C::PRIMARY_CONTAINER, Color::WHITE, 0.08)
        }
        _ => C::PRIMARY_CONTAINER,
    };
    button::Style {
            snap: false,
        background: Some(Background::Color(bg)),
        text_color: C::ON_PRIMARY_CONTAINER,
        border: Border {
            color: C::PRIMARY,
            width: 2.0,
            radius: Radius::from(12.0),
        },
        shadow: flat(),
    }
}

/// Accelerating device card (green container, 2px green border).
pub fn device_card_acced(_theme: &Theme, status: Status) -> button::Style {
    let bg = match status {
        Status::Hovered | Status::Pressed => {
            state_layer(C::GREEN_CONTAINER, Color::WHITE, 0.08)
        }
        _ => C::GREEN_CONTAINER,
    };
    button::Style {
            snap: false,
        background: Some(Background::Color(bg)),
        text_color: C::ON_GREEN_CONTAINER,
        border: Border {
            color: C::GREEN,
            width: 2.0,
            radius: Radius::from(12.0),
        },
        shadow: flat(),
    }
}

/// Generic interactive list row (menu item): transparent, 8% hover layer.
pub fn list_row(_theme: &Theme, status: Status) -> button::Style {
    let bg = match status {
        Status::Hovered => Some(state_layer(C::SURFACE, C::ON_SURFACE, 0.08)),
        Status::Pressed => Some(state_layer(C::SURFACE, C::ON_SURFACE, 0.10)),
        _ => None,
    };
    bs(bg, C::ON_SURFACE, rounded(8.0))
}

/// Selected item of a menu / list (primary container).
pub fn picker_selected_row(_theme: &Theme, status: Status) -> button::Style {
    let bg = match status {
        Status::Hovered => state_layer(C::PRIMARY_CONTAINER, Color::WHITE, 0.08),
        _ => C::PRIMARY_CONTAINER,
    };
    bs(
        Some(bg),
        C::ON_PRIMARY_CONTAINER,
        rounded(8.0),
    )
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

/// M3 filled card: fills the window below the tab strip, so no own corner
/// radius is needed — the DWM window rounding clips the corners anyway.
pub fn card(_theme: &Theme) -> container::Style {
    container::Style {
            snap: false,
        background: Some(Background::Color(C::SURFACE_CONTAINER)),
        text_color: Some(C::ON_SURFACE),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::from(0.0),
        },
        shadow: flat(),
    }
}

/// Frameless-window background: only the header strip paints the base
/// SURFACE; the card below fills the rest. Keeping the base translucent
/// wastes a full-window fill every frame (~1/3 of the raster cost).
pub fn window(_theme: &Theme) -> container::Style {
    container::Style {
        snap: false,
        background: None,
        text_color: Some(C::ON_SURFACE),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::from(0.0),
        },
        shadow: flat(),
    }
}
/// Single-pixel section divider.
pub fn divider(_theme: &Theme) -> container::Style {
    container::Style {
            snap: false,
        background: Some(Background::Color(C::OUTLINE_VARIANT)),
        text_color: None,
        border: border::rounded(0.0),
        shadow: flat(),
    }
}

/// Assisting warning banner (e.g. mobile concurrency notice).
pub fn hint(_theme: &Theme) -> container::Style {
    container::Style {
            snap: false,
        background: Some(Background::Color(state_layer(
            C::SURFACE,
            C::ERROR,
            0.12,
        ))),
        text_color: Some(C::ERROR),
        border: Border {
            color: Color {
                a: 0.4,
                ..C::ERROR_CONTAINER
            },
            width: 1.0,
            radius: Radius::from(12.0),
        },
        shadow: flat(),
    }
}

/// Translucent chip (auto-adapts to the card background beneath it).
#[allow(dead_code)]
pub fn chip(_theme: &Theme) -> container::Style {
    container::Style {
            snap: false,
        background: Some(Background::Color(Color {
            a: 0.14,
            r: 1.0,
            g: 1.0,
            b: 1.0,
        })),
        text_color: None,
        border: border::rounded(999.0),
        shadow: flat(),
    }
}

/// Circular avatar clip.
pub fn avatar(_theme: &Theme) -> container::Style {
    container::Style {
            snap: false,
        background: Some(Background::Color(C::SURFACE_HIGH)),
        text_color: None,
        border: Border {
            color: C::OUTLINE,
            width: 1.0,
            radius: Radius::from(999.0),
        },
        shadow: flat(),
    }
}

// ---------------------------------------------------------------------------
// Text fields & pick lists (M3 outlined)
// ---------------------------------------------------------------------------

use text_input::Status as InputStatus;

pub fn text_field(_theme: &Theme, status: InputStatus) -> text_input::Style {
    let (border, w, bg) = match status {
        InputStatus::Focused { is_hovered: _ } => (C::PRIMARY, 2.0, C::SURFACE_LOW),
        InputStatus::Hovered => (C::INVERSE, 1.0, C::SURFACE_LOW),
        InputStatus::Disabled => (
            Color {
                a: 0.12,
                ..C::ON_SURFACE
            },
            1.0,
            state_layer(C::SURFACE_LOW, C::ON_SURFACE, 0.06),
        ),
        InputStatus::Active => (C::OUTLINE, 1.0, C::SURFACE_LOW),
    };
    text_input::Style {
        background: Background::Color(bg),
        border: Border {
            color: border,
            width: w,
            radius: Radius::from(8.0),
        },
        placeholder: Color {
            a: 0.64,
            ..C::ON_SURFACE_VARIANT
        },
        value: C::ON_SURFACE,
        selection: Color {
            a: 0.30,
            ..C::PRIMARY
        },
    }
}

use pick_list::Status as PickStatus;

pub fn picker(_theme: &Theme, status: PickStatus) -> pick_list::Style {
    let (border, w, bg) = match status {
        PickStatus::Opened { is_hovered: _ } => (C::PRIMARY, 2.0, C::SURFACE_HIGH),
        PickStatus::Hovered => (C::INVERSE, 1.0, C::SURFACE_LOW),
        PickStatus::Disabled => (
            Color {
                a: 0.12,
                ..C::ON_SURFACE
            },
            1.0,
            state_layer(C::SURFACE_LOW, C::ON_SURFACE, 0.06),
        ),
        PickStatus::Active => (C::OUTLINE, 1.0, C::SURFACE_LOW),
    };
    pick_list::Style {
        text_color: C::ON_SURFACE,
        placeholder_color: Color {
            a: 0.64,
            ..C::ON_SURFACE_VARIANT
        },
        handle_color: C::PRIMARY,
        background: Background::Color(bg),
        border: Border {
            color: border,
            width: w,
            radius: Radius::from(8.0),
        },
    }
}

// ---------------------------------------------------------------------------
// Scrollbars
// ---------------------------------------------------------------------------

pub fn scroll_bar(_theme: &Theme, _status: scrollable::Status) -> scrollable::Style {
    let rail = scrollable::Rail {
        background: None,
        border: border::rounded(999.0),
        scroller: scrollable::Scroller {
            background: Background::Color(C::OUTLINE),
            border: border::rounded(999.0),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: {
            scrollable::Rail {
                background: None,
                border: border::rounded(999.0),
                scroller: scrollable::Scroller {
                    background: Background::Color(Color::TRANSPARENT),
                    border: border::rounded(999.0),
                },
            }
        },
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Color::TRANSPARENT.into(),
            border: border::rounded(999.0),
            shadow: flat(),
            icon: C::ON_SURFACE,
        },
    }
}




