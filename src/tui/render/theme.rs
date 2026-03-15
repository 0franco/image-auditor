use ratatui::style::Color;

// ─── Backgrounds ─────────────────────────────────────────────────────────────
pub const BG_DEEP: Color    = Color::Rgb(10, 10, 18);
pub const BG_SURFACE: Color = Color::Rgb(15, 18, 30);
pub const BG_ELEVATED: Color = Color::Rgb(22, 26, 45);
pub const BG_HIGHLIGHT: Color = Color::Rgb(28, 36, 72);

// ─── Borders ─────────────────────────────────────────────────────────────────
pub const BORDER_DIM: Color     = Color::Rgb(35, 42, 80);
pub const BORDER_DEFAULT: Color = Color::Rgb(55, 65, 115);

// ─── Text ────────────────────────────────────────────────────────────────────
pub const TEXT_PRIMARY: Color   = Color::Rgb(210, 215, 240);
pub const TEXT_SECONDARY: Color = Color::Rgb(145, 155, 195);
pub const TEXT_MUTED: Color     = Color::Rgb(75, 85, 125);
pub const TEXT_LABEL: Color     = Color::Rgb(100, 115, 160);

// ─── Accents ─────────────────────────────────────────────────────────────────
pub const ACCENT_CYAN: Color  = Color::Rgb(99, 219, 255);
pub const ACCENT_GREEN: Color = Color::Rgb(99, 235, 180);
pub const ACCENT_BLUE: Color  = Color::Rgb(120, 170, 245);

// ─── Severity ────────────────────────────────────────────────────────────────
pub const SEV_ERROR: Color   = Color::Rgb(235, 75,  75);
pub const SEV_WARNING: Color = Color::Rgb(235, 178, 55);
pub const SEV_INFO: Color    = Color::Rgb(99,  175, 237);

// ─── Diff ────────────────────────────────────────────────────────────────────
pub const DIFF_DEL_BG: Color = Color::Rgb(65, 16, 16);
pub const DIFF_DEL_FG: Color = Color::Rgb(240, 128, 128);
pub const DIFF_ADD_BG: Color = Color::Rgb(16, 52, 24);
pub const DIFF_ADD_FG: Color = Color::Rgb(128, 238, 158);
pub const DIFF_HDR_BG: Color = Color::Rgb(30, 35, 65);
pub const DIFF_HDR_FG: Color = Color::Rgb(140, 160, 210);
