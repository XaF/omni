use atty;
use lazy_static::lazy_static;

use crate::internal::env::ENV;

pub const BLACK: &str = "30";
pub const RED: &str = "31";
pub const GREEN: &str = "32";
pub const YELLOW: &str = "33";
pub const BLUE: &str = "34";
pub const MAGENTA: &str = "35";
pub const CYAN: &str = "36";
pub const WHITE: &str = "37";
pub const LIGHT_BLACK: &str = "90";
pub const LIGHT_RED: &str = "91";
pub const LIGHT_GREEN: &str = "92";
pub const LIGHT_YELLOW: &str = "93";
pub const LIGHT_BLUE: &str = "94";
pub const LIGHT_MAGENTA: &str = "95";
pub const LIGHT_CYAN: &str = "96";
pub const LIGHT_WHITE: &str = "97";

pub const BOLD: &str = "1";
pub const RESET_BOLD: &str = "22";
pub const DIM: &str = "2";
pub const RESET_DIM: &str = "22";
pub const ITALIC: &str = "3";
pub const RESET_ITALIC: &str = "23";
pub const UNDERLINED: &str = "4";
pub const RESET_UNDERLINED: &str = "24";
pub const BLINK: &str = "5";
pub const RESET_BLINK: &str = "25";
pub const RAPID_BLINK: &str = "6";
pub const RESET_RAPID_BLINK: &str = "26";
pub const REVERSE: &str = "7";
pub const RESET_REVERSE: &str = "27";
pub const HIDDEN: &str = "8";
pub const RESET_HIDDEN: &str = "28";
pub const STRIKETHROUGH: &str = "9";
pub const RESET_STRIKETHROUGH: &str = "29";

pub const DEFAULT: &str = "39";
pub const RESET: &str = "0";

lazy_static! {
    pub static ref COLORS_ENABLED: bool = enable_colors();
}

// http://bixense.com/clicolors/
fn enable_colors() -> bool {
    if let Some(_) = std::env::var_os("NO_COLOR") {
        return false;
    }
    if let Some(_) = std::env::var_os("CLICOLOR_FORCE") {
        return true;
    }
    atty::is(atty::Stream::Stdout) || atty::is(atty::Stream::Stderr)
}

pub trait StringColor {
    fn colorize(&self, color_code: &str) -> String;
    fn force_colorize(&self, color_code: &str) -> String;
    fn noncolormodifier(&self, modifier: &str, cancel_modifier: &str) -> String;
    fn force_noncolormodifier(&self, modifier: &str, cancel_modifier: &str) -> String;

    fn black(&self) -> String;
    fn red(&self) -> String;
    fn green(&self) -> String;
    fn yellow(&self) -> String;
    fn blue(&self) -> String;
    fn magenta(&self) -> String;
    fn cyan(&self) -> String;
    fn white(&self) -> String;
    fn force_black(&self) -> String;
    fn force_red(&self) -> String;
    fn force_green(&self) -> String;
    fn force_yellow(&self) -> String;
    fn force_blue(&self) -> String;
    fn force_magenta(&self) -> String;
    fn force_cyan(&self) -> String;
    fn force_white(&self) -> String;

    fn light_black(&self) -> String;
    fn light_red(&self) -> String;
    fn light_green(&self) -> String;
    fn light_yellow(&self) -> String;
    fn light_blue(&self) -> String;
    fn light_magenta(&self) -> String;
    fn light_cyan(&self) -> String;
    fn light_white(&self) -> String;
    fn force_light_black(&self) -> String;
    fn force_light_red(&self) -> String;
    fn force_light_green(&self) -> String;
    fn force_light_yellow(&self) -> String;
    fn force_light_blue(&self) -> String;
    fn force_light_magenta(&self) -> String;
    fn force_light_cyan(&self) -> String;
    fn force_light_white(&self) -> String;

    fn bold(&self) -> String;
    fn dim(&self) -> String;
    fn italic(&self) -> String;
    fn underline(&self) -> String;
    fn blink(&self) -> String;
    fn rapid_blink(&self) -> String;
    fn reverse(&self) -> String;
    fn hidden(&self) -> String;
    fn strikethrough(&self) -> String;
    fn force_bold(&self) -> String;
    fn force_dim(&self) -> String;
    fn force_italic(&self) -> String;
    fn force_underline(&self) -> String;
    fn force_blink(&self) -> String;
    fn force_rapid_blink(&self) -> String;
    fn force_reverse(&self) -> String;
    fn force_hidden(&self) -> String;
    fn force_strikethrough(&self) -> String;

    fn normal(&self) -> String;
    fn force_normal(&self) -> String;
}

// Implement the extension trait for the existing type
impl StringColor for String {
    fn colorize(&self, color: &str) -> String {
        if *COLORS_ENABLED {
            self.force_colorize(color)
        } else {
            self.to_string()
        }
    }

    fn force_colorize(&self, color: &str) -> String {
        format!("\x1B[{}m{}\x1B[39m", color, self)
    }

    fn noncolormodifier(&self, modifier: &str, cancel_modifier: &str) -> String {
        if *COLORS_ENABLED {
            self.force_noncolormodifier(modifier, cancel_modifier)
        } else {
            self.to_string()
        }
    }

    fn force_noncolormodifier(&self, modifier: &str, cancel_modifier: &str) -> String {
        format!("\x1B[{}m{}\x1B[{}m", modifier, self, cancel_modifier)
    }

    fn black(&self) -> String {
        self.colorize(BLACK)
    }

    fn force_black(&self) -> String {
        self.force_colorize(BLACK)
    }

    fn red(&self) -> String {
        self.colorize(RED)
    }

    fn force_red(&self) -> String {
        self.force_colorize(RED)
    }

    fn green(&self) -> String {
        self.colorize(GREEN)
    }

    fn force_green(&self) -> String {
        self.force_colorize(GREEN)
    }

    fn yellow(&self) -> String {
        self.colorize(YELLOW)
    }

    fn force_yellow(&self) -> String {
        self.force_colorize(YELLOW)
    }

    fn blue(&self) -> String {
        self.colorize(BLUE)
    }

    fn force_blue(&self) -> String {
        self.force_colorize(BLUE)
    }

    fn magenta(&self) -> String {
        self.colorize(MAGENTA)
    }

    fn force_magenta(&self) -> String {
        self.force_colorize(MAGENTA)
    }

    fn cyan(&self) -> String {
        self.colorize(CYAN)
    }

    fn force_cyan(&self) -> String {
        self.force_colorize(CYAN)
    }

    fn white(&self) -> String {
        self.colorize(WHITE)
    }

    fn force_white(&self) -> String {
        self.force_colorize(WHITE)
    }

    fn light_black(&self) -> String {
        self.colorize(LIGHT_BLACK)
    }

    fn force_light_black(&self) -> String {
        self.force_colorize(LIGHT_BLACK)
    }

    fn light_red(&self) -> String {
        self.colorize(LIGHT_RED)
    }

    fn force_light_red(&self) -> String {
        self.force_colorize(LIGHT_RED)
    }

    fn light_green(&self) -> String {
        self.colorize(LIGHT_GREEN)
    }

    fn force_light_green(&self) -> String {
        self.force_colorize(LIGHT_GREEN)
    }

    fn light_yellow(&self) -> String {
        self.colorize(LIGHT_YELLOW)
    }

    fn force_light_yellow(&self) -> String {
        self.force_colorize(LIGHT_YELLOW)
    }

    fn light_blue(&self) -> String {
        self.colorize(LIGHT_BLUE)
    }

    fn force_light_blue(&self) -> String {
        self.force_colorize(LIGHT_BLUE)
    }

    fn light_magenta(&self) -> String {
        self.colorize(LIGHT_MAGENTA)
    }

    fn force_light_magenta(&self) -> String {
        self.force_colorize(LIGHT_MAGENTA)
    }

    fn light_cyan(&self) -> String {
        self.colorize(LIGHT_CYAN)
    }

    fn force_light_cyan(&self) -> String {
        self.force_colorize(LIGHT_CYAN)
    }

    fn light_white(&self) -> String {
        self.colorize(LIGHT_WHITE)
    }

    fn force_light_white(&self) -> String {
        self.force_colorize(LIGHT_WHITE)
    }

    fn bold(&self) -> String {
        self.noncolormodifier(BOLD, RESET_BOLD)
    }

    fn force_bold(&self) -> String {
        self.force_noncolormodifier(BOLD, RESET_BOLD)
    }

    fn dim(&self) -> String {
        self.noncolormodifier(DIM, RESET_DIM)
    }

    fn force_dim(&self) -> String {
        self.force_noncolormodifier(DIM, RESET_DIM)
    }

    fn italic(&self) -> String {
        self.noncolormodifier(ITALIC, RESET_ITALIC)
    }

    fn force_italic(&self) -> String {
        self.force_noncolormodifier(ITALIC, RESET_ITALIC)
    }

    fn underline(&self) -> String {
        self.noncolormodifier(UNDERLINED, RESET_UNDERLINED)
    }

    fn force_underline(&self) -> String {
        self.force_noncolormodifier(UNDERLINED, RESET_UNDERLINED)
    }

    fn blink(&self) -> String {
        self.noncolormodifier(BLINK, RESET_BLINK)
    }

    fn force_blink(&self) -> String {
        self.force_noncolormodifier(BLINK, RESET_BLINK)
    }

    fn rapid_blink(&self) -> String {
        self.noncolormodifier(RAPID_BLINK, RESET_RAPID_BLINK)
    }

    fn force_rapid_blink(&self) -> String {
        self.force_noncolormodifier(RAPID_BLINK, RESET_RAPID_BLINK)
    }

    fn reverse(&self) -> String {
        self.noncolormodifier(REVERSE, RESET_REVERSE)
    }

    fn force_reverse(&self) -> String {
        self.force_noncolormodifier(REVERSE, RESET_REVERSE)
    }

    fn hidden(&self) -> String {
        self.noncolormodifier(HIDDEN, RESET_HIDDEN)
    }

    fn force_hidden(&self) -> String {
        self.force_noncolormodifier(HIDDEN, RESET_HIDDEN)
    }

    fn strikethrough(&self) -> String {
        self.noncolormodifier(STRIKETHROUGH, RESET_STRIKETHROUGH)
    }

    fn force_strikethrough(&self) -> String {
        self.force_noncolormodifier(STRIKETHROUGH, RESET_STRIKETHROUGH)
    }

    fn normal(&self) -> String {
        if ENV.interactive_shell {
            format!("\x1B[{}m{}", RESET, self)
        } else {
            self.to_string()
        }
    }

    fn force_normal(&self) -> String {
        format!("\x1B[{}m{}", RESET, self)
    }
}
