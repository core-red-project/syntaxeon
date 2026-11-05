//! ANSI styling; every method is the identity when color is off.

#[derive(Clone, Copy)]
pub struct Style(pub bool);

impl Style {
    fn wrap(self, code: &str, s: &str) -> String {
        if self.0 {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn bold(self, s: &str) -> String {
        self.wrap("1", s)
    }
    pub fn cyan(self, s: &str) -> String {
        self.wrap("36", s)
    }
    pub fn yellow(self, s: &str) -> String {
        self.wrap("33", s)
    }
    pub fn dim(self, s: &str) -> String {
        self.wrap("2", s)
    }
    pub fn red(self, s: &str) -> String {
        self.wrap("1;31", s)
    }
    pub fn blue(self, s: &str) -> String {
        self.wrap("34", s)
    }
}
