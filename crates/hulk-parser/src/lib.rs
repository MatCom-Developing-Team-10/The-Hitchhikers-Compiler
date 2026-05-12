//! LALRpop-based parser for the HULK language.
//!
//! The grammar lives in `grammar.lalrpop` and is generated into `grammar.rs`
//! by the build script. The generated module is re-exported here.

use lalrpop_util::lalrpop_mod;

lalrpop_mod!(pub grammar);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integer() {
        let parsed = grammar::NumParser::new().parse("42").unwrap();
        assert_eq!(parsed, 42);
    }
}
