// SPDX-License-Identifier: GPL-3.0-or-later
//
//! The search query language: parsing what a person types, compiling what FTS5 accepts.
//!
//! The grammar is bloop's (`server/bleep/src/query/grammar.pest`, at the commit pinned in
//! docs/upstream-sources.toml), reduced to what a prose index can answer. Nothing is copied: bloop
//! parses with pest into a Tantivy query, and this is a hand-written parser targeting FTS5 `MATCH`.
//! docs/index-engine-decision.md records why FTS5 stayed.
//!
//! # What this replaces
//!
//! The previous implementation stripped every non-alphanumeric character, quoted each remaining word
//! and joined them with `OR`. So a quoted phrase was torn into separate words, an exclusion was
//! silently dropped, and -- once `heading` became an indexed column -- `heading:policy` searched for the
//! literal word "heading", which is worse than ignoring it.
//!
//! # The one guarantee that matters
//!
//! No input may produce a `MATCH` expression SQLite rejects. That is not a style preference: FTS5
//! answers a malformed expression with a runtime error, so a stray quote in a search box would surface
//! as a failed query rather than as no results. Measured, each of these is an FTS5 error --
//! `""`, `"AND"`, `"("`, `"\""`, `":"`, `"*"`, `"NOT alpha"`, and any unknown column name -- so the
//! compiler is built from a parsed tree and never from the user's bytes, and every refusal is a typed
//! error carrying a sentence a person can act on.
//!
//! # Deliberately absent
//!
//! `regex` -- FTS5 is a tokenised inverted index and cannot walk its term dictionary. Decided out of
//! scope in an earlier cycle: bloop needs it because developers grep code, and this product indexes
//! prose. `repo:`, `org:`, `branch:` -- bloop searches git repositories. `symbol:` needs code
//! intelligence. `open:` is a UI flag. `path:` is not an FTS5 term and does not need to be, because the
//! search layer already filters by path in SQL after fusion.

use std::fmt::Write as _;

/// A parsed query.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A single word, optionally scoped to a column, optionally a prefix match.
    Term {
        column: Option<String>,
        text: String,
        prefix: bool,
    },
    /// A quoted run of words that must appear in order.
    Phrase {
        column: Option<String>,
        text: String,
    },
    All(Vec<Node>),
    Any(Vec<Node>),
}

/// A parsed query and the exclusions that apply to it.
///
/// Exclusions are held beside the positive tree rather than inside it, because FTS5's `NOT` is BINARY:
/// measured, `NOT alpha` is a syntax error while `beta NOT alpha` is not. There is no expression for
/// "everything", so an exclusion is only meaningful against a positive set -- which makes a query of
/// only exclusions a refusal rather than something to compile.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub include: Node,
    pub exclude: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum QueryError {
    #[error("type something to search for")]
    Empty,
    #[error("a search needs something to look for, not only terms to exclude")]
    OnlyExclusions,
    #[error("unknown field {field:?}; searchable fields are {available}")]
    UnknownColumn { field: String, available: String },
    #[error("a quotation mark is not closed")]
    UnterminatedPhrase,
    #[error("a bracket is not closed")]
    UnclosedGroup,
    #[error("{0:?} needs a word on both sides")]
    DanglingOperator(String),
    #[error("an empty pair of brackets has nothing to search for")]
    EmptyGroup,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Phrase(String),
    Field(String),
    And,
    Or,
    Not,
    Open,
    Close,
}

/// Split the input into tokens, keeping quoted runs whole.
fn tokenize(input: &str) -> Result<Vec<Token>, QueryError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut word = String::new();

    // A word is flushed on any boundary. `field:` is recognised at flush time rather than while
    // scanning, so a colon inside a word (a URL, a time) does not turn the prefix into a field.
    macro_rules! flush {
        () => {
            if !word.is_empty() {
                let taken = std::mem::take(&mut word);
                match taken.to_ascii_uppercase().as_str() {
                    "AND" => tokens.push(Token::And),
                    "OR" => tokens.push(Token::Or),
                    "NOT" => tokens.push(Token::Not),
                    _ => tokens.push(Token::Word(taken)),
                }
            }
        };
    }

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                flush!();
                let mut phrase = String::new();
                let mut closed = false;
                while let Some(inner) = chars.next() {
                    if inner == '"' {
                        // A doubled quote is one literal quote, the way SQL and FTS5 both spell it.
                        if chars.peek() == Some(&'"') {
                            chars.next();
                            phrase.push('"');
                            continue;
                        }
                        closed = true;
                        break;
                    }
                    phrase.push(inner);
                }
                if !closed {
                    return Err(QueryError::UnterminatedPhrase);
                }
                // An empty pair of quotes is a no-op rather than an error: a person closing a quote
                // they never filled has not asked for anything, and refusing would be pedantic.
                if !phrase.trim().is_empty() {
                    tokens.push(Token::Phrase(phrase));
                }
            }
            ':' => {
                if !word.is_empty() {
                    tokens.push(Token::Field(std::mem::take(&mut word)));
                    continue;
                }
                // `heading : policy` -- the space already flushed the field name as a word, so the
                // previous token is promoted. FTS5 itself accepts that spacing, measured, so a person
                // who types it should not silently get a search for the word "heading".
                match tokens.last() {
                    Some(Token::Word(name)) => {
                        let name = name.clone();
                        tokens.pop();
                        tokens.push(Token::Field(name));
                    }
                    // A colon naming nothing is a typo; dropped, because the rest is still answerable.
                    _ => continue,
                }
            }
            '(' => {
                flush!();
                tokens.push(Token::Open);
            }
            ')' => {
                flush!();
                tokens.push(Token::Close);
            }
            '-' if word.is_empty() => {
                // A leading hyphen excludes, as it does in every search box. Inside a word it is part
                // of the word, so `well-known` stays one token.
                tokens.push(Token::Not);
            }
            ch if ch.is_whitespace() => flush!(),
            ch => word.push(ch),
        }
    }
    flush!();
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    columns: &'a [&'a str],
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.position);
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn check_column(&self, field: &str) -> Result<String, QueryError> {
        // FTS5 answers an unknown column name with a runtime error naming it, so the name has to be
        // checked before it ever reaches a statement.
        let lower = field.to_ascii_lowercase();
        if self.columns.iter().any(|known| *known == lower) {
            Ok(lower)
        } else {
            Err(QueryError::UnknownColumn {
                field: field.to_string(),
                available: self.columns.join(", "),
            })
        }
    }

    /// `a OR b` -- the loosest binding, so it is parsed outermost.
    fn parse_any(&mut self) -> Result<(Node, Vec<Node>), QueryError> {
        if matches!(self.peek(), Some(Token::Or)) {
            // Otherwise the empty left branch is discarded and `OR b` silently becomes `b`.
            return Err(QueryError::DanglingOperator("OR".into()));
        }
        let (first, mut excluded) = self.parse_all()?;
        let mut branches = vec![first];
        while matches!(self.peek(), Some(Token::Or)) {
            self.next();
            if self.peek().is_none() {
                return Err(QueryError::DanglingOperator("OR".into()));
            }
            let (next, mut more) = self.parse_all()?;
            branches.push(next);
            excluded.append(&mut more);
        }
        Ok((
            if branches.len() == 1 {
                branches.pop().expect("one branch")
            } else {
                Node::Any(branches)
            },
            excluded,
        ))
    }

    /// A run of terms, which are ANDed. `AND` may be written or left implicit.
    fn parse_all(&mut self) -> Result<(Node, Vec<Node>), QueryError> {
        let mut branches = Vec::new();
        let mut excluded = Vec::new();

        loop {
            match self.peek() {
                None | Some(Token::Or) | Some(Token::Close) => break,
                Some(Token::And) => {
                    self.next();
                    if matches!(self.peek(), None | Some(Token::Or) | Some(Token::Close)) {
                        return Err(QueryError::DanglingOperator("AND".into()));
                    }
                    continue;
                }
                Some(Token::Not) => {
                    self.next();
                    if matches!(self.peek(), None | Some(Token::Or) | Some(Token::Close)) {
                        return Err(QueryError::DanglingOperator("NOT".into()));
                    }
                    let (node, mut nested) = self.parse_atom()?;
                    excluded.push(node);
                    excluded.append(&mut nested);
                    continue;
                }
                _ => {
                    let (node, mut nested) = self.parse_atom()?;
                    branches.push(node);
                    excluded.append(&mut nested);
                }
            }
        }

        let node = match branches.len() {
            0 => return Ok((Node::All(Vec::new()), excluded)),
            1 => branches.pop().expect("one branch"),
            _ => Node::All(branches),
        };
        Ok((node, excluded))
    }

    fn parse_atom(&mut self) -> Result<(Node, Vec<Node>), QueryError> {
        match self.next().cloned() {
            Some(Token::Open) => {
                let (inner, excluded) = self.parse_any()?;
                match self.next() {
                    Some(Token::Close) => {}
                    _ => return Err(QueryError::UnclosedGroup),
                }
                if matches!(&inner, Node::All(parts) if parts.is_empty()) {
                    return Err(QueryError::EmptyGroup);
                }
                Ok((inner, excluded))
            }
            Some(Token::Word(text)) => Ok((word_node(None, text), Vec::new())),
            Some(Token::Phrase(text)) => Ok((Node::Phrase { column: None, text }, Vec::new())),
            Some(Token::Field(field)) => {
                let column = self.check_column(&field)?;
                match self.next().cloned() {
                    Some(Token::Word(text)) => Ok((word_node(Some(column), text), Vec::new())),
                    Some(Token::Phrase(text)) => Ok((
                        Node::Phrase {
                            column: Some(column),
                            text,
                        },
                        Vec::new(),
                    )),
                    // `heading:` with nothing after it scopes nothing. Treated as the field name having
                    // been a stray word rather than an error, since the rest of the query still stands.
                    _ => Ok((Node::All(Vec::new()), Vec::new())),
                }
            }
            Some(Token::And) | Some(Token::Or) => {
                Err(QueryError::DanglingOperator("AND/OR".into()))
            }
            Some(Token::Not) => Err(QueryError::DanglingOperator("NOT".into())),
            Some(Token::Close) => Err(QueryError::UnclosedGroup),
            None => Err(QueryError::Empty),
        }
    }
}

/// A trailing `*` asks for a prefix match, which FTS5 supports natively.
fn word_node(column: Option<String>, text: String) -> Node {
    if let Some(stripped) = text.strip_suffix('*') {
        if !stripped.is_empty() {
            return Node::Term {
                column,
                text: stripped.to_string(),
                prefix: true,
            };
        }
    }
    Node::Term {
        column,
        text,
        prefix: false,
    }
}

/// Parse a person's query. `columns` is the searchable field list, lower-cased.
pub fn parse(input: &str, columns: &[&str]) -> Result<Query, QueryError> {
    let mut tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(QueryError::Empty);
    }
    // "and", "or" and "not" are words a document can be about. When a query has no searchable token at
    // all, the operators were the query: someone looking for the word "and" gets it rather than a
    // syntax error. Quoting at compile time is what keeps them from becoming operators again.
    let has_searchable = tokens
        .iter()
        .any(|token| matches!(token, Token::Word(_) | Token::Phrase(_)));
    if !has_searchable {
        tokens = tokens
            .into_iter()
            .map(|token| match token {
                Token::And => Token::Word("and".into()),
                Token::Or => Token::Word("or".into()),
                Token::Not => Token::Word("not".into()),
                other => other,
            })
            .collect();
    }
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        columns,
    };
    let (include, exclude) = parser.parse_any()?;
    if parser.position < tokens.len() {
        // Anything left over is an unbalanced bracket; the loops above stop at a `)` they did not open.
        return Err(QueryError::UnclosedGroup);
    }
    let include = prune(include);
    match &include {
        Node::All(parts) if parts.is_empty() => {
            if exclude.is_empty() {
                Err(QueryError::Empty)
            } else {
                Err(QueryError::OnlyExclusions)
            }
        }
        _ => Ok(Query {
            include,
            exclude: exclude
                .into_iter()
                .map(prune)
                .filter(|node| !is_empty(node))
                .collect(),
        }),
    }
}

fn is_empty(node: &Node) -> bool {
    matches!(node, Node::All(parts) | Node::Any(parts) if parts.is_empty())
}

/// Drop the empty placeholders a stray field name or quote pair leaves behind.
fn prune(node: Node) -> Node {
    match node {
        Node::All(parts) => {
            let kept: Vec<Node> = parts
                .into_iter()
                .map(prune)
                .filter(|n| !is_empty(n))
                .collect();
            if kept.len() == 1 {
                kept.into_iter().next().expect("one")
            } else {
                Node::All(kept)
            }
        }
        Node::Any(parts) => {
            let kept: Vec<Node> = parts
                .into_iter()
                .map(prune)
                .filter(|n| !is_empty(n))
                .collect();
            if kept.len() == 1 {
                kept.into_iter().next().expect("one")
            } else {
                Node::Any(kept)
            }
        }
        other => other,
    }
}

/// Quote a term as an FTS5 string, doubling embedded quotes.
///
/// Every term is quoted whether or not it needs to be. An unquoted term that happens to read as an FTS5
/// keyword -- a document about the word "and", or "near" -- would otherwise become an operator.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

fn compile_node(node: &Node, out: &mut String) {
    match node {
        Node::Term {
            column,
            text,
            prefix,
        } => {
            if let Some(column) = column {
                let _ = write!(out, "{column} : ");
            }
            let _ = write!(out, "{}{}", quoted(text), if *prefix { "*" } else { "" });
        }
        Node::Phrase { column, text } => {
            if let Some(column) = column {
                let _ = write!(out, "{column} : ");
            }
            let _ = write!(out, "{}", quoted(text));
        }
        Node::All(parts) | Node::Any(parts) => {
            let joiner = if matches!(node, Node::All(_)) {
                " AND "
            } else {
                " OR "
            };
            out.push('(');
            for (index, part) in parts.iter().enumerate() {
                if index > 0 {
                    out.push_str(joiner);
                }
                compile_node(part, out);
            }
            out.push(')');
        }
    }
}

/// Compile a parsed query into an FTS5 `MATCH` expression.
pub fn compile(query: &Query) -> String {
    let mut out = String::new();
    compile_node(&query.include, &mut out);
    // One binary NOT per exclusion, left-associated. FTS5 has no unary negation, which is why an
    // exclusion cannot stand alone and `parse` refuses a query made only of them.
    for excluded in &query.exclude {
        out.push_str(" NOT ");
        compile_node(excluded, &mut out);
    }
    out
}

/// Parse and compile in one step.
pub fn to_match_expression(input: &str, columns: &[&str]) -> Result<String, QueryError> {
    Ok(compile(&parse(input, columns)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLUMNS: &[&str] = &["content", "heading"];

    fn expression(input: &str) -> String {
        to_match_expression(input, COLUMNS).expect("should compile")
    }

    fn error(input: &str) -> QueryError {
        to_match_expression(input, COLUMNS).expect_err("should be refused")
    }

    #[test]
    fn a_bare_word_is_quoted() {
        // Quoted even though it needs no quoting: a document about the word "and" or "near" would
        // otherwise have its search term read as an operator.
        assert_eq!(expression("policy"), "\"policy\"");
        assert_eq!(expression("and"), "\"and\"");
    }

    #[test]
    fn several_words_are_all_required() {
        // The previous implementation ORed them, so a two-word search returned everything matching
        // either. Requiring both is what a person means by typing two words.
        assert_eq!(expression("revenue policy"), "(\"revenue\" AND \"policy\")");
    }

    #[test]
    fn explicit_operators_work_and_or_binds_loosest() {
        assert_eq!(expression("a AND b"), "(\"a\" AND \"b\")");
        assert_eq!(expression("a OR b"), "(\"a\" OR \"b\")");
        // `a b OR c` is `(a AND b) OR c`, not `a AND (b OR c)`.
        assert_eq!(expression("a b OR c"), "((\"a\" AND \"b\") OR \"c\")");
    }

    #[test]
    fn brackets_override_precedence() {
        assert_eq!(expression("a (b OR c)"), "(\"a\" AND (\"b\" OR \"c\"))");
    }

    #[test]
    fn a_phrase_stays_whole() {
        // The old implementation stripped the quotes and split the words apart, which silently turned
        // a phrase search into an OR of its words.
        assert_eq!(
            expression("\"revenue recognition\""),
            "\"revenue recognition\""
        );
    }

    #[test]
    fn a_field_scopes_a_term_or_a_phrase() {
        assert_eq!(expression("heading:policy"), "heading : \"policy\"");
        assert_eq!(expression("heading : policy"), "heading : \"policy\"");
        assert_eq!(
            expression("heading:\"revenue recognition\""),
            "heading : \"revenue recognition\""
        );
        assert_eq!(expression("CONTENT:policy"), "content : \"policy\"");
    }

    #[test]
    fn an_unknown_field_is_refused_by_name() {
        // FTS5 answers an unknown column with a runtime error, so it is caught before a statement runs.
        match error("author:someone") {
            QueryError::UnknownColumn { field, available } => {
                assert_eq!(field, "author");
                assert_eq!(available, "content, heading");
            }
            other => panic!("expected UnknownColumn, got {other:?}"),
        }
    }

    #[test]
    fn a_hyphen_excludes_and_attaches_to_the_positive_query() {
        // FTS5's NOT is binary -- `NOT alpha` is a syntax error -- so an exclusion is compiled against
        // the positive set rather than on its own.
        assert_eq!(expression("revenue -draft"), "\"revenue\" NOT \"draft\"");
        assert_eq!(expression("revenue NOT draft"), "\"revenue\" NOT \"draft\"");
    }

    #[test]
    fn a_hyphen_inside_a_word_is_part_of_the_word() {
        assert_eq!(expression("well-known"), "\"well-known\"");
    }

    #[test]
    fn a_query_of_only_exclusions_is_refused() {
        // There is no FTS5 expression for "everything", so this cannot be compiled -- and a person who
        // typed it should be told, not handed an empty result set.
        assert_eq!(error("-draft"), QueryError::OnlyExclusions);
        assert_eq!(error("NOT draft"), QueryError::OnlyExclusions);
    }

    #[test]
    fn a_trailing_star_is_a_prefix_search() {
        assert_eq!(expression("polic*"), "\"polic\"*");
        // A lone star is not a prefix of anything; FTS5 errors on it, so it stays a literal term.
        assert_eq!(expression("*"), "\"*\"");
    }

    #[test]
    fn an_empty_query_is_refused() {
        assert_eq!(error(""), QueryError::Empty);
        assert_eq!(error("   "), QueryError::Empty);
    }

    #[test]
    fn unbalanced_syntax_is_refused_with_a_readable_reason() {
        assert_eq!(error("\"unclosed"), QueryError::UnterminatedPhrase);
        assert_eq!(error("(unclosed"), QueryError::UnclosedGroup);
        assert_eq!(error("unopened)"), QueryError::UnclosedGroup);
        assert_eq!(error("()"), QueryError::EmptyGroup);
        assert!(matches!(error("a AND"), QueryError::DanglingOperator(_)));
        assert!(matches!(error("OR b"), QueryError::DanglingOperator(_)));
    }

    #[test]
    fn stray_punctuation_does_not_derail_the_rest() {
        // A colon with no field before it, and a field with nothing after it, are typos rather than
        // errors: the rest of the query is still answerable.
        assert_eq!(expression(":policy"), "\"policy\"");
        assert_eq!(expression("policy heading:"), "\"policy\"");
        assert_eq!(expression("policy \"\""), "\"policy\"");
    }

    #[test]
    fn an_embedded_quote_is_doubled_rather_than_ending_the_term() {
        let compiled = expression("\"say \"\"hello\"\"\"");
        assert!(compiled.contains("\"\""), "got {compiled}");
    }
}

/// Every expression this compiler produces must be one SQLite accepts.
///
/// The guarantee, not a nicety: FTS5 answers a malformed expression with a runtime error, so a stray
/// character in a search box would surface as a failed query rather than as no results. These run the
/// compiled output through a real FTS5 table, because the only authority on what FTS5 accepts is FTS5.
#[cfg(test)]
mod fts5_acceptance {
    use super::*;
    use rusqlite::Connection;

    const COLUMNS: &[&str] = &["content", "heading"];

    fn table() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE chunks (id INTEGER PRIMARY KEY, content TEXT, heading TEXT);
                 CREATE VIRTUAL TABLE chunks_fts USING fts5(
                     content, heading, content='chunks', content_rowid='id');
                 INSERT INTO chunks VALUES
                    (1, 'alpha beta gamma and near', 'Intro'),
                    (2, 'beta delta', 'Alpha notes'),
                    (3, 'epsilon', 'Other');
                 INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild');",
            )
            .unwrap();
        connection
    }

    /// Run a compiled expression; returns the matching rowids or the FTS5 error.
    fn run(connection: &Connection, expression: &str) -> Result<Vec<i64>, String> {
        let mut statement = connection
            .prepare("SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1 ORDER BY rowid")
            .unwrap();
        let rows = statement
            .query_map([expression], |row| row.get::<_, i64>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(rows)
    }

    /// Inputs a person could plausibly type, including the hostile and the malformed.
    const INPUTS: &[&str] = &[
        "alpha",
        "alpha beta",
        "alpha OR epsilon",
        "alpha AND beta",
        "alpha -delta",
        "alpha NOT delta",
        "(alpha OR epsilon) beta",
        "\"alpha beta\"",
        "heading:alpha",
        "heading : alpha",
        "heading:\"alpha notes\"",
        "content:beta heading:other",
        "alph*",
        "and",
        "or",
        "not",
        "near",
        "well-known",
        "a AND b OR c d",
        "\"say \"\"hello\"\"\"",
        ":alpha",
        "alpha heading:",
        "alpha \"\"",
        "*",
        "alpha*beta",
        "**",
        "---",
        "alpha)",
        "((alpha))",
        "alpha ( beta )",
        "€ alpha",
        "?!;,.",
        "alpha; DROP TABLE chunks; --",
        "\"; DROP TABLE chunks; --\"",
        "alpha OR OR beta",
        "NEAR(alpha beta)",
        "{content}:alpha",
        "alpha\"beta",
    ];

    #[test]
    fn no_input_produces_an_expression_fts5_rejects() {
        let connection = table();
        for input in INPUTS {
            match to_match_expression(input, COLUMNS) {
                // A refusal is a correct outcome: the point is never to hand SQLite something invalid.
                Err(_) => continue,
                Ok(expression) => {
                    let result = run(&connection, &expression);
                    assert!(
                        result.is_ok(),
                        "input {input:?} compiled to {expression:?} which FTS5 rejected: {:?}",
                        result.unwrap_err()
                    );
                }
            }
        }
    }

    #[test]
    fn a_query_cannot_reach_beyond_the_match_expression() {
        // The expression is bound as a parameter, so SQL injection is not the risk -- the risk is a
        // crafted string becoming FTS5 syntax. Either way the table must survive and the words must be
        // treated as words.
        let connection = table();
        for input in [
            "alpha; DROP TABLE chunks; --",
            "\"; DROP TABLE chunks; --\"",
        ] {
            if let Ok(expression) = to_match_expression(input, COLUMNS) {
                let _ = run(&connection, &expression);
            }
        }
        let survived: i64 = connection
            .query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(survived, 3);
    }

    #[test]
    fn the_compiled_meaning_is_the_intended_one() {
        // Compiling to something FTS5 merely accepts is not enough; it has to mean what was asked.
        let connection = table();
        let matched = |input: &str| {
            let expression = to_match_expression(input, COLUMNS).expect("compiles");
            run(&connection, &expression).expect("accepted")
        };

        assert_eq!(matched("alpha"), vec![1, 2], "row 2 has it in its heading");
        assert_eq!(
            matched("alpha beta"),
            vec![1, 2],
            "both terms, in either column"
        );
        assert_eq!(matched("gamma beta"), vec![1], "only row 1 has both");
        assert_eq!(matched("heading:alpha"), vec![2], "scoped to the heading");
        assert_eq!(matched("content:alpha"), vec![1], "scoped to the body");
        assert_eq!(matched("beta -delta"), vec![1], "row 2 is excluded");
        assert_eq!(matched("\"alpha beta\""), vec![1], "adjacent, in order");
        assert_eq!(
            matched("\"beta alpha\""),
            Vec::<i64>::new(),
            "order matters"
        );
        assert_eq!(matched("epsilon OR gamma"), vec![1, 3]);
        assert_eq!(matched("alph*"), vec![1, 2]);
        // The words that are also operators.
        assert_eq!(matched("and"), vec![1], "searching for the word \"and\"");
        assert_eq!(matched("near"), vec![1], "and for \"near\"");
    }
}
