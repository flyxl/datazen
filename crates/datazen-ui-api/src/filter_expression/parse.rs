use std::collections::BTreeSet;

use crate::filter_expression::types::{
    FilterExpression, FilterExpressionLogic, FilterExpressionOperator, FilterExpressionValue,
    ParseFailure,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keyword {
    And,
    Or,
    Is,
    Not,
}

impl Keyword {
    fn as_str(self) -> &'static str {
        match self {
            Self::And => "AND",
            Self::Or => "OR",
            Self::Is => "IS",
            Self::Not => "NOT",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Identifier { value: String, position: usize },
    Literal { value: FilterExpressionValue, position: usize },
    Operator { value: FilterExpressionOperator, position: usize },
    Keyword { value: Keyword, position: usize },
    LeftParen { position: usize },
    RightParen { position: usize },
    Eof { position: usize },
}

impl Token {
    fn position(&self) -> usize {
        match self {
            Self::Identifier { position, .. }
            | Self::Literal { position, .. }
            | Self::Operator { position, .. }
            | Self::Keyword { position, .. }
            | Self::LeftParen { position }
            | Self::RightParen { position }
            | Self::Eof { position } => *position,
        }
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn char_at(input: &str, index: usize) -> Option<(char, usize)> {
    input[index..].chars().next().map(|character| (character, character.len_utf8()))
}

fn read_string(input: &str, start: usize) -> Result<(String, usize), ParseFailure> {
    let mut index = start + 1;
    let mut value = String::new();
    while index < input.len() {
        let byte = input.as_bytes()[index];
        if byte == b'\'' {
            if input.as_bytes().get(index + 1) == Some(&b'\'') {
                value.push('\'');
                index += 2;
                continue;
            }
            return Ok((value, index + 1));
        }
        if byte == b'\\' {
            let escaped = input.as_bytes().get(index + 1).copied();
            match escaped {
                None => return Err(ParseFailure::new("Unclosed string literal.", start)),
                Some(b'\'') => value.push('\''),
                Some(b'\\') => value.push('\\'),
                Some(b'n') => value.push('\n'),
                Some(b'r') => value.push('\r'),
                Some(b't') => value.push('\t'),
                Some(_) => return Err(ParseFailure::new("Unsupported string escape.", index)),
            }
            index += 2;
            continue;
        }
        if byte == b'\n' || byte == b'\r' {
            return Err(ParseFailure::new(
                "String literals cannot contain an unescaped line break.",
                index,
            ));
        }
        let (character, length) = char_at(input, index).unwrap_or(('\0', 1));
        value.push(character);
        index += length;
    }
    Err(ParseFailure::new("Unclosed string literal.", start))
}

fn read_quoted_identifier(input: &str, start: usize) -> Result<(String, usize), ParseFailure> {
    let mut index = start + 1;
    let mut value = String::new();
    while index < input.len() {
        let byte = input.as_bytes()[index];
        if byte == b'"' {
            if input.as_bytes().get(index + 1) == Some(&b'"') {
                value.push('"');
                index += 2;
                continue;
            }
            return Ok((value, index + 1));
        }
        if byte == b'\n' || byte == b'\r' {
            return Err(ParseFailure::new(
                "Quoted column names cannot contain an unescaped line break.",
                index,
            ));
        }
        let (character, length) = char_at(input, index).unwrap_or(('\0', 1));
        value.push(character);
        index += length;
    }
    Err(ParseFailure::new("Unclosed quoted column name.", start))
}

fn read_number(input: &str, start: usize) -> Result<Option<(f64, usize)>, ParseFailure> {
    let bytes = input.as_bytes();
    let mut index = start;
    if bytes.get(index).is_some_and(|byte| *byte == b'+' || *byte == b'-') {
        index += 1;
    }
    let Some(first) = bytes.get(index).copied() else {
        return Ok(None);
    };
    if !first.is_ascii_digit() {
        return Ok(None);
    }
    if first == b'0' {
        index += 1;
    } else {
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    let integer_end = index;
    if bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    } else {
        index = integer_end;
    }
    let fraction_end = index;
    if matches!(bytes.get(index).copied(), Some(b'e' | b'E')) {
        let mut cursor = index + 1;
        if bytes.get(cursor).is_some_and(|byte| *byte == b'+' || *byte == b'-') {
            cursor += 1;
        }
        if bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
            while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
            }
            index = cursor;
        } else {
            index = fraction_end;
        }
    }
    let raw = &input[start..index];
    match raw.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(Some((value, index))),
        _ => Err(ParseFailure::new("Numeric literal is out of range.", start)),
    }
}

fn tokenize(input: &str) -> Result<Vec<Token>, ParseFailure> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < input.len() {
        let byte = input.as_bytes()[index];
        let (character, length) = char_at(input, index).unwrap_or(('\0', 1));
        if character.is_whitespace() {
            index += length;
            continue;
        }
        match byte {
            b'\'' => {
                let (value, next) = read_string(input, index)?;
                tokens.push(Token::Literal { value: FilterExpressionValue::Str(value), position: index });
                index = next;
            }
            b'"' => {
                let (value, next) = read_quoted_identifier(input, index)?;
                tokens.push(Token::Identifier { value, position: index });
                index = next;
            }
            b'(' => {
                tokens.push(Token::LeftParen { position: index });
                index += 1;
            }
            b')' => {
                tokens.push(Token::RightParen { position: index });
                index += 1;
            }
            b'=' => {
                if input.as_bytes().get(index + 1) == Some(&b'=') {
                    return Err(ParseFailure::new("Only = is supported for equality.", index));
                }
                tokens.push(Token::Operator { value: FilterExpressionOperator::Eq, position: index });
                index += 1;
            }
            b'!' => {
                if input.as_bytes().get(index + 1) != Some(&b'=') {
                    return Err(ParseFailure::new("Only != is supported.", index));
                }
                tokens.push(Token::Operator { value: FilterExpressionOperator::Ne, position: index });
                index += 2;
            }
            b'<' => {
                let next = input.as_bytes().get(index + 1).copied();
                if next == Some(b'>') {
                    return Err(ParseFailure::new("Only the supported comparison operators are allowed.", index));
                }
                if next == Some(b'=') {
                    tokens.push(Token::Operator { value: FilterExpressionOperator::Lte, position: index });
                    index += 2;
                } else {
                    tokens.push(Token::Operator { value: FilterExpressionOperator::Lt, position: index });
                    index += 1;
                }
            }
            b'>' => {
                if input.as_bytes().get(index + 1) == Some(&b'=') {
                    tokens.push(Token::Operator { value: FilterExpressionOperator::Gte, position: index });
                    index += 2;
                } else {
                    tokens.push(Token::Operator { value: FilterExpressionOperator::Gt, position: index });
                    index += 1;
                }
            }
            b'+' | b'-' | b'0'..=b'9' => {
                let Some((value, next)) = read_number(input, index)? else {
                    return Err(ParseFailure::new("Expected a numeric literal.", index));
                };
                tokens.push(Token::Literal { value: FilterExpressionValue::Num(value), position: index });
                index = next;
            }
            _ if is_identifier_start(byte) => {
                let start = index;
                index += 1;
                while input.as_bytes().get(index).is_some_and(|byte| is_identifier_part(*byte)) {
                    index += 1;
                }
                let word = &input[start..index];
                let upper = word.to_ascii_uppercase();
                match upper.as_str() {
                    "TRUE" => tokens.push(Token::Literal { value: FilterExpressionValue::Bool(true), position: start }),
                    "FALSE" => tokens.push(Token::Literal { value: FilterExpressionValue::Bool(false), position: start }),
                    "NULL" => tokens.push(Token::Literal { value: FilterExpressionValue::Null, position: start }),
                    "AND" => tokens.push(Token::Keyword { value: Keyword::And, position: start }),
                    "OR" => tokens.push(Token::Keyword { value: Keyword::Or, position: start }),
                    "IS" => tokens.push(Token::Keyword { value: Keyword::Is, position: start }),
                    "NOT" => tokens.push(Token::Keyword { value: Keyword::Not, position: start }),
                    _ => tokens.push(Token::Identifier { value: word.to_owned(), position: start }),
                }
            }
            _ => return Err(ParseFailure::new("Unsupported token in filter expression.", index)),
        }
    }
    tokens.push(Token::Eof { position: input.len() });
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    columns: BTreeSet<String>,
    index: usize,
}

impl Parser {
    fn current(&self) -> Token {
        self.tokens.get(self.index).cloned().unwrap_or(Token::Eof { position: 0 })
    }

    fn matches_left_paren(&mut self) -> bool {
        let matched = matches!(self.tokens.get(self.index), Some(Token::LeftParen { .. }));
        if matched {
            self.index += 1;
        }
        matched
    }

    fn matches_keyword(&mut self, keyword: Keyword) -> bool {
        let matched = matches!(self.tokens.get(self.index), Some(Token::Keyword { value, .. }) if *value == keyword);
        if matched {
            self.index += 1;
        }
        matched
    }

    fn expect_identifier(&mut self, message: &str) -> Result<(String, usize), ParseFailure> {
        match self.current() {
            Token::Identifier { value, position } => {
                self.index += 1;
                Ok((value, position))
            }
            token => Err(ParseFailure::new(message, token.position())),
        }
    }

    fn expect_right_paren(&mut self, message: &str) -> Result<(), ParseFailure> {
        match self.current() {
            Token::RightParen { .. } => {
                self.index += 1;
                Ok(())
            }
            token => Err(ParseFailure::new(message, token.position())),
        }
    }

    fn expect_null(&mut self, message: &str) -> Result<(), ParseFailure> {
        match self.current() {
            Token::Literal { value: FilterExpressionValue::Null, .. } => {
                self.index += 1;
                Ok(())
            }
            token => Err(ParseFailure::new(message, token.position())),
        }
    }

    fn parse(&mut self) -> Result<FilterExpression, ParseFailure> {
        let expression = self.parse_or()?;
        let token = self.current();
        if !matches!(token, Token::Eof { .. }) {
            return Err(ParseFailure::new("Unexpected token after the filter expression.", token.position()));
        }
        Ok(expression)
    }

    fn parse_or(&mut self) -> Result<FilterExpression, ParseFailure> {
        let mut expression = self.parse_and()?;
        while self.matches_keyword(Keyword::Or) {
            let right = self.parse_and()?;
            expression = FilterExpression::logical(FilterExpressionLogic::Or, expression, right);
        }
        Ok(expression)
    }

    fn parse_and(&mut self) -> Result<FilterExpression, ParseFailure> {
        let mut expression = self.parse_primary()?;
        while self.matches_keyword(Keyword::And) {
            let right = self.parse_primary()?;
            expression = FilterExpression::logical(FilterExpressionLogic::And, expression, right);
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<FilterExpression, ParseFailure> {
        if self.matches_left_paren() {
            let expression = self.parse_or()?;
            self.expect_right_paren("Expected closing parenthesis.")?;
            return Ok(expression);
        }
        self.parse_condition()
    }

    fn parse_condition(&mut self) -> Result<FilterExpression, ParseFailure> {
        let (column, position) = self.expect_identifier("Expected an allowed column name.")?;
        if !self.columns.contains(&column) {
            return Err(ParseFailure::new(&format!("Unknown filter column: {column}."), position));
        }
        let operator = self.parse_operator()?;
        if !operator.is_comparison() {
            return Ok(FilterExpression::condition(&column, operator, FilterExpressionValue::Null));
        }
        match self.current() {
            Token::Literal { value, .. } => {
                self.index += 1;
                Ok(FilterExpression::condition(&column, operator, value))
            }
            token => Err(ParseFailure::new("Expected a string, number, boolean, or NULL value.", token.position())),
        }
    }

    fn parse_operator(&mut self) -> Result<FilterExpressionOperator, ParseFailure> {
        match self.current() {
            Token::Operator { value, .. } => {
                self.index += 1;
                Ok(value)
            }
            _ if self.matches_keyword(Keyword::Is) => {
                let not = self.matches_keyword(Keyword::Not);
                self.expect_null("Expected NULL after IS or IS NOT.")?;
                Ok(if not { FilterExpressionOperator::IsNotNull } else { FilterExpressionOperator::IsNull })
            }
            token => Err(ParseFailure::new(
                "Expected one of =, !=, >, >=, <, <=, IS NULL, or IS NOT NULL.",
                token.position(),
            )),
        }
    }
}

pub fn parse_filter_expression(
    input: &str,
    columns: &[String],
) -> Result<FilterExpression, ParseFailure> {
    let allowed: BTreeSet<String> = columns.iter().cloned().collect();
    let tokens = tokenize(input)?;
    let mut parser = Parser { tokens, columns: allowed, index: 0 };
    parser.parse()
}

pub fn token_positions(input: &str) -> Result<Vec<usize>, ParseFailure> {
    Ok(tokenize(input)?.iter().map(Token::position).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_simple_comparison_builds_a_condition_ast() {
        assert_eq!(
            parse_filter_expression("age >= 18", &cols(&["age"])),
            Ok(FilterExpression::condition(
                "age",
                FilterExpressionOperator::Gte,
                FilterExpressionValue::Num(18.0)
            ))
        );
    }

    #[test]
    fn and_binds_tighter_than_or() {
        let tree = parse_filter_expression(
            "a = 1 OR b = 2 AND c = 3",
            &cols(&["a", "b", "c"]),
        );
        let expected = FilterExpression::logical(
            FilterExpressionLogic::Or,
            FilterExpression::condition("a", FilterExpressionOperator::Eq, FilterExpressionValue::Num(1.0)),
            FilterExpression::logical(
                FilterExpressionLogic::And,
                FilterExpression::condition("b", FilterExpressionOperator::Eq, FilterExpressionValue::Num(2.0)),
                FilterExpression::condition("c", FilterExpressionOperator::Eq, FilterExpressionValue::Num(3.0)),
            ),
        );
        assert_eq!(tree, Ok(expected));
    }

    #[test]
    fn parentheses_override_precedence() {
        let tree = parse_filter_expression("(a = 1 OR b = 2) AND c = 3", &cols(&["a", "b", "c"]));
        assert!(matches!(tree, Ok(FilterExpression::Logical { operator: FilterExpressionLogic::And, .. })));
    }

    #[test]
    fn null_and_boolean_literals_are_data() {
        assert_eq!(
            parse_filter_expression("flag = TRUE", &cols(&["flag"])),
            Ok(FilterExpression::condition("flag", FilterExpressionOperator::Eq, FilterExpressionValue::Bool(true)))
        );
        assert_eq!(
            parse_filter_expression("flag = false", &cols(&["flag"])),
            Ok(FilterExpression::condition("flag", FilterExpressionOperator::Eq, FilterExpressionValue::Bool(false)))
        );
        assert_eq!(
            parse_filter_expression("flag = NULL", &cols(&["flag"])),
            Ok(FilterExpression::condition("flag", FilterExpressionOperator::Eq, FilterExpressionValue::Null))
        );
    }

    #[test]
    fn is_null_and_is_not_null_need_no_literal() {
        for (text, operator) in [
            ("name IS NULL", FilterExpressionOperator::IsNull),
            ("name IS NOT NULL", FilterExpressionOperator::IsNotNull),
        ] {
            assert_eq!(
                parse_filter_expression(text, &cols(&["name"])),
                Ok(FilterExpression::condition("name", operator, FilterExpressionValue::Null))
            );
        }
    }

    #[test]
    fn quoted_identifiers_and_escaped_strings() {
        assert_eq!(
            parse_filter_expression("\"order\"\"item\" = 'It''s'", &cols(&["order\"item"])),
            Ok(FilterExpression::condition(
                "order\"item",
                FilterExpressionOperator::Eq,
                FilterExpressionValue::Str("It's".to_owned())
            ))
        );
        assert_eq!(
            parse_filter_expression("name = 'a\\nb\\tc\\\\d\\'e'", &cols(&["name"])),
            Ok(FilterExpression::condition(
                "name",
                FilterExpressionOperator::Eq,
                FilterExpressionValue::Str("a\nb\tc\\d'e".to_owned())
            ))
        );
    }

    #[test]
    fn numbers_accept_signs_decimals_and_exponents() {
        assert_eq!(
            parse_filter_expression("x = -12.5e+2", &cols(&["x"])),
            Ok(FilterExpression::condition("x", FilterExpressionOperator::Eq, FilterExpressionValue::Num(-1250.0)))
        );
        assert_eq!(
            parse_filter_expression("x = 0", &cols(&["x"])),
            Ok(FilterExpression::condition("x", FilterExpressionOperator::Eq, FilterExpressionValue::Num(0.0)))
        );
    }

    #[test]
    fn unknown_columns_and_missing_operands_report_positions() {
        assert_eq!(
            parse_filter_expression("zz = 1", &cols(&["a"])),
            Err(ParseFailure::new("Unknown filter column: zz.", 0))
        );
        assert_eq!(
            parse_filter_expression("a = ", &cols(&["a"])),
            Err(ParseFailure::new("Expected a string, number, boolean, or NULL value.", 4))
        );
        assert_eq!(
            parse_filter_expression("a IS 1", &cols(&["a"])),
            Err(ParseFailure::new("Expected NULL after IS or IS NOT.", 5))
        );
    }

    #[test]
    fn unsupported_operators_and_tokens_fail_at_their_position() {
        assert_eq!(parse_filter_expression("a == 1", &cols(&["a"])), Err(ParseFailure::new("Only = is supported for equality.", 2)));
        assert!(parse_filter_expression("a != 1", &cols(&["a"])).is_ok());
        assert_eq!(parse_filter_expression("a ! 1", &cols(&["a"])), Err(ParseFailure::new("Only != is supported.", 2)));
        assert_eq!(parse_filter_expression("a <> 1", &cols(&["a"])), Err(ParseFailure::new("Only the supported comparison operators are allowed.", 2)));
        assert_eq!(parse_filter_expression("a = 1 @", &cols(&["a"])), Err(ParseFailure::new("Unsupported token in filter expression.", 6)));
    }

    #[test]
    fn malformed_strings_and_identifiers_fail() {
        assert_eq!(parse_filter_expression("a = 'oops", &cols(&["a"])), Err(ParseFailure::new("Unclosed string literal.", 4)));
        assert_eq!(parse_filter_expression("a = 'x\\q'", &cols(&["a"])), Err(ParseFailure::new("Unsupported string escape.", 6)));
        assert_eq!(parse_filter_expression("\"oops = 1", &cols(&["a"])), Err(ParseFailure::new("Unclosed quoted column name.", 0)));
    }

    #[test]
    fn trailing_tokens_and_unclosed_parentheses_fail() {
        assert_eq!(
            parse_filter_expression("a = 1 b = 2", &cols(&["a", "b"])),
            Err(ParseFailure::new("Unexpected token after the filter expression.", 6))
        );
        assert_eq!(
            parse_filter_expression("(a = 1", &cols(&["a"])),
            Err(ParseFailure::new("Expected closing parenthesis.", 6))
        );
    }

    #[test]
    fn token_positions_are_visible() {
        assert_eq!(token_positions("a = 1"), Ok(vec![0, 2, 4, 5]));
    }

    #[test]
    fn empty_input_fails_at_eof() {
        assert_eq!(
            parse_filter_expression("", &cols(&["a"])),
            Err(ParseFailure::new("Expected an allowed column name.", 0))
        );
    }
}
