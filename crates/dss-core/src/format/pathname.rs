//! DSS pathname parsing and manipulation.

/// A parsed DSS pathname with its A-F parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pathname {
    pub full: String,
    pub parts: [String; 6],
}

impl Pathname {
    /// Parse a DSS pathname string into its parts.
    /// Format: `/A/B/C/D/E/F/`
    pub fn parse(pathname: &str) -> Self {
        let trimmed = pathname.trim_matches('/');
        let segments: Vec<&str> = trimmed.split('/').collect();
        let mut parts = [
            String::new(), String::new(), String::new(),
            String::new(), String::new(), String::new(),
        ];
        for (i, seg) in segments.iter().enumerate().take(6) {
            parts[i] = seg.to_string();
        }
        Pathname {
            full: pathname.to_string(),
            parts,
        }
    }

    /// Form a pathname from its A-F parts.
    pub fn from_parts(a: &str, b: &str, c: &str, d: &str, e: &str, f: &str) -> Self {
        let full = format!("/{a}/{b}/{c}/{d}/{e}/{f}/");
        Pathname {
            full,
            parts: [
                a.to_string(), b.to_string(), c.to_string(),
                d.to_string(), e.to_string(), f.to_string(),
            ],
        }
    }

    pub fn a(&self) -> &str { &self.parts[0] }
    pub fn b(&self) -> &str { &self.parts[1] }
    pub fn c(&self) -> &str { &self.parts[2] }
    pub fn d(&self) -> &str { &self.parts[3] }
    pub fn e(&self) -> &str { &self.parts[4] }
    pub fn f(&self) -> &str { &self.parts[5] }
}

impl std::fmt::Display for Pathname {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pathname() {
        let p = Pathname::parse("/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/");
        assert_eq!(p.a(), "BASIN");
        assert_eq!(p.b(), "LOC");
        assert_eq!(p.c(), "FLOW");
        assert_eq!(p.d(), "01JAN2020");
        assert_eq!(p.e(), "1HOUR");
        assert_eq!(p.f(), "OBS");
    }

    #[test]
    fn test_parse_empty_parts() {
        let p = Pathname::parse("/A/B/C///F/");
        assert_eq!(p.d(), "");
        assert_eq!(p.e(), "");
        assert_eq!(p.f(), "F");
    }

    #[test]
    fn test_from_parts() {
        let p = Pathname::from_parts("A", "B", "C", "D", "E", "F");
        assert_eq!(p.full, "/A/B/C/D/E/F/");
    }

    #[test]
    fn test_display() {
        let p = Pathname::parse("/X/Y/Z///W/");
        assert_eq!(format!("{p}"), "/X/Y/Z///W/");
    }
}
