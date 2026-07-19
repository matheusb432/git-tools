//! Lightweight Rust token scanning for architecture ownership boundaries.

const SQLITE_OPERATION_IDENTIFIERS: [&str; 9] = [
    "prepare",
    "prepare_cached",
    "query_row",
    "execute",
    "execute_batch",
    "transaction",
    "transaction_with_behavior",
    "unchecked_transaction",
    "unchecked_transaction_with_behavior",
];

/// A forbidden production-CLI Git boundary marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GitBoundaryViolation {
    GitRunnerPort,
    RetiredGitShim,
    ReceiverRun,
    DirectGitLaunch,
}

impl GitBoundaryViolation {
    pub(super) const fn description(self) -> &'static str {
        match self {
            Self::GitRunnerPort => "reference the application GitRunner port",
            Self::RetiredGitShim => "use the retired crate::git shim",
            Self::ReceiverRun => "call a receiver method named run",
            Self::DirectGitLaunch => "launch git directly",
        }
    }
}

/// Finds forbidden Git boundary markers in the production portion of one Rust source file.
pub(super) fn git_boundary_violations(source: &str) -> Vec<GitBoundaryViolation> {
    let tokens = production_tokens(source);
    let checks = [
        (
            GitBoundaryViolation::GitRunnerPort,
            contains_identifier(&tokens, "GitRunner"),
        ),
        (
            GitBoundaryViolation::RetiredGitShim,
            contains_sequence(
                &tokens,
                &[
                    ExpectedToken::Identifier("crate"),
                    ExpectedToken::Punctuation(':'),
                    ExpectedToken::Punctuation(':'),
                    ExpectedToken::Identifier("git"),
                ],
            ),
        ),
        (
            GitBoundaryViolation::ReceiverRun,
            contains_sequence(
                &tokens,
                &[
                    ExpectedToken::Punctuation('.'),
                    ExpectedToken::Identifier("run"),
                ],
            ),
        ),
        (
            GitBoundaryViolation::DirectGitLaunch,
            contains_direct_git_launch(&tokens),
        ),
    ];

    checks
        .into_iter()
        .filter_map(|(violation, present)| present.then_some(violation))
        .collect()
}

/// Finds top-level method names in a named production trait.
pub(super) fn trait_method_names_top_level<'source>(
    source: &'source str,
    trait_name: &str,
) -> Option<Vec<&'source str>> {
    let tokens = production_tokens(source);
    let body = named_trait_body(&tokens, trait_name)?;
    let mut delimiters = Vec::new();
    let mut method_names = Vec::new();

    for (index, token) in body.iter().copied().enumerate() {
        if delimiters.is_empty()
            && token == Token::Identifier("fn")
            && let Some(Token::Identifier(method_name)) = body.get(index + 1)
        {
            method_names.push(*method_name);
        }
        if !update_delimiters(&mut delimiters, token) {
            break;
        }
    }

    Some(method_names)
}

/// Finds forbidden `SQLite` receiver operations in production Rust source.
pub(super) fn sqlite_operation_identifiers(source: &str) -> Vec<&'static str> {
    let tokens = production_tokens(source);
    SQLITE_OPERATION_IDENTIFIERS
        .into_iter()
        .filter(|identifier| {
            contains_sequence(
                &tokens,
                &[
                    ExpectedToken::Punctuation('.'),
                    ExpectedToken::Identifier(identifier),
                ],
            )
        })
        .collect()
}

fn named_trait_body<'tokens, 'source>(
    tokens: &'tokens [Token<'source>],
    trait_name: &str,
) -> Option<&'tokens [Token<'source>]> {
    let mut delimiters = Vec::new();
    let mut index = 0;

    while index + 1 < tokens.len() {
        if delimiters.is_empty()
            && tokens[index] == Token::Identifier("trait")
            && tokens[index + 1] == Token::Identifier(trait_name)
        {
            let body_start = trait_body_start(tokens, index + 2)?;
            let body_end = delimiter_end(tokens, body_start)?;
            return Some(&tokens[body_start + 1..body_end - 1]);
        }

        if !update_delimiters(&mut delimiters, tokens[index]) {
            return None;
        }
        index += 1;
    }

    None
}

fn trait_body_start(tokens: &[Token<'_>], start: usize) -> Option<usize> {
    let mut delimiters = Vec::new();

    for (index, token) in tokens.iter().copied().enumerate().skip(start) {
        if delimiters.is_empty() && token == Token::Punctuation('{') {
            return Some(index);
        }
        let previous = index
            .checked_sub(1)
            .and_then(|index| tokens.get(index).copied());
        if !update_trait_header_delimiters(&mut delimiters, token, previous) {
            return None;
        }
    }

    None
}

fn update_trait_header_delimiters(
    delimiters: &mut Vec<char>,
    token: Token<'_>,
    previous: Option<Token<'_>>,
) -> bool {
    match token {
        Token::Punctuation('<') if delimiters.iter().all(|delimiter| *delimiter == '<') => {
            delimiters.push('<');
            true
        }
        Token::Punctuation('>')
            if previous != Some(Token::Punctuation('-')) && delimiters.last() == Some(&'<') =>
        {
            delimiters.pop();
            true
        }
        _ => update_delimiters(delimiters, token),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'source> {
    Identifier(&'source str),
    Punctuation(char),
    StringLiteral(&'source str),
}

#[derive(Debug, Clone, Copy)]
enum ExpectedToken<'source> {
    Identifier(&'source str),
    Punctuation(char),
    StringLiteral(&'source str),
}

impl ExpectedToken<'_> {
    fn matches(self, actual: Token<'_>) -> bool {
        match (self, actual) {
            (Self::Identifier(expected), Token::Identifier(actual))
            | (Self::StringLiteral(expected), Token::StringLiteral(actual)) => expected == actual,
            (Self::Punctuation(expected), Token::Punctuation(actual)) => expected == actual,
            _ => false,
        }
    }
}

fn contains_identifier(tokens: &[Token<'_>], expected: &str) -> bool {
    tokens
        .iter()
        .any(|token| matches!(token, Token::Identifier(actual) if *actual == expected))
}

fn contains_direct_git_launch(tokens: &[Token<'_>]) -> bool {
    contains_named_git_launch(tokens, "Command")
        || tokens.windows(3).any(|window| {
            let [
                Token::Identifier("Command"),
                Token::Identifier("as"),
                Token::Identifier(alias),
            ] = window
            else {
                return false;
            };
            contains_named_git_launch(tokens, alias)
        })
}

fn contains_named_git_launch(tokens: &[Token<'_>], receiver: &str) -> bool {
    contains_sequence(
        tokens,
        &[
            ExpectedToken::Identifier(receiver),
            ExpectedToken::Punctuation(':'),
            ExpectedToken::Punctuation(':'),
            ExpectedToken::Identifier("new"),
            ExpectedToken::Punctuation('('),
            ExpectedToken::StringLiteral("git"),
        ],
    )
}

fn contains_sequence(tokens: &[Token<'_>], expected: &[ExpectedToken<'_>]) -> bool {
    tokens.windows(expected.len()).any(|window| {
        expected
            .iter()
            .zip(window)
            .all(|(expected, actual)| expected.matches(*actual))
    })
}

fn production_tokens(source: &str) -> Vec<Token<'_>> {
    let tokens = tokenize(source);
    if has_file_cfg_test(&tokens) {
        return Vec::new();
    }

    if let Some(suffix_start) = test_module_suffix_start(&tokens) {
        return tokens[..suffix_start].to_vec();
    }

    tokens
}

fn has_file_cfg_test(tokens: &[Token<'_>]) -> bool {
    let cfg_test = [
        ExpectedToken::Punctuation('#'),
        ExpectedToken::Punctuation('!'),
        ExpectedToken::Punctuation('['),
        ExpectedToken::Identifier("cfg"),
        ExpectedToken::Punctuation('('),
        ExpectedToken::Identifier("test"),
        ExpectedToken::Punctuation(')'),
        ExpectedToken::Punctuation(']'),
    ];
    let mut index = 0;

    while starts_sequence(
        &tokens[index..],
        &[
            ExpectedToken::Punctuation('#'),
            ExpectedToken::Punctuation('!'),
            ExpectedToken::Punctuation('['),
        ],
    ) {
        let Some(attribute_end) = delimiter_end(tokens, index + 2) else {
            return false;
        };
        if attribute_end == index + cfg_test.len()
            && starts_sequence(&tokens[index..attribute_end], &cfg_test)
        {
            return true;
        }
        index = attribute_end;
    }

    false
}

fn test_module_suffix_start(tokens: &[Token<'_>]) -> Option<usize> {
    let mut delimiters = Vec::new();

    for index in 0..tokens.len() {
        if delimiters.is_empty() && starts_test_module(&tokens[index..]) {
            let body_start = index + test_module_prefix().len();
            let suffix_end = match tokens[body_start] {
                Token::Punctuation(';') => Some(body_start + 1),
                Token::Punctuation('{') => delimiter_end(tokens, body_start),
                _ => None,
            };
            if suffix_end == Some(tokens.len()) {
                return Some(index);
            }
        }
        update_delimiters(&mut delimiters, tokens[index]);
    }

    None
}

fn starts_test_module(tokens: &[Token<'_>]) -> bool {
    let expected = test_module_prefix();

    tokens.len() > expected.len()
        && starts_sequence(tokens, &expected)
        && matches!(tokens[expected.len()], Token::Punctuation('{' | ';'))
}

fn test_module_prefix() -> [ExpectedToken<'static>; 9] {
    [
        ExpectedToken::Punctuation('#'),
        ExpectedToken::Punctuation('['),
        ExpectedToken::Identifier("cfg"),
        ExpectedToken::Punctuation('('),
        ExpectedToken::Identifier("test"),
        ExpectedToken::Punctuation(')'),
        ExpectedToken::Punctuation(']'),
        ExpectedToken::Identifier("mod"),
        ExpectedToken::Identifier("tests"),
    ]
}

fn starts_sequence(tokens: &[Token<'_>], expected: &[ExpectedToken<'_>]) -> bool {
    tokens.len() >= expected.len()
        && expected
            .iter()
            .zip(tokens)
            .all(|(expected, actual)| expected.matches(*actual))
}

fn delimiter_end(tokens: &[Token<'_>], open_index: usize) -> Option<usize> {
    let mut delimiters = Vec::new();
    for (index, token) in tokens.iter().copied().enumerate().skip(open_index) {
        if !update_delimiters(&mut delimiters, token) {
            return None;
        }
        if delimiters.is_empty() {
            return Some(index + 1);
        }
    }
    None
}

fn update_delimiters(delimiters: &mut Vec<char>, token: Token<'_>) -> bool {
    match token {
        Token::Punctuation(open @ ('(' | '[' | '{')) => delimiters.push(open),
        Token::Punctuation(close @ (')' | ']' | '}')) => {
            let expected = match close {
                ')' => '(',
                ']' => '[',
                '}' => '{',
                _ => unreachable!(),
            };
            if delimiters.pop() != Some(expected) {
                return false;
            }
        }
        _ => {}
    }
    true
}

fn tokenize(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = skip_line_comment(bytes, index + 2);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = skip_block_comment(bytes, index + 2);
            }
            b'r' if raw_string_hashes(bytes, index).is_some() => {
                let hashes = raw_string_hashes(bytes, index).expect("raw string was identified");
                let (content, next) = raw_string(source, index, hashes);
                tokens.push(Token::StringLiteral(content));
                index = next;
            }
            b'"' => {
                let (content, next) = quoted_string(source, index);
                tokens.push(Token::StringLiteral(content));
                index = next;
            }
            b'b' if bytes.get(index + 1) == Some(&b'"') => {
                index = quoted_string(source, index + 1).1;
            }
            b'b' if bytes.get(index + 1) == Some(&b'r')
                && raw_string_hashes(bytes, index + 1).is_some() =>
            {
                let hashes =
                    raw_string_hashes(bytes, index + 1).expect("raw byte string was identified");
                index = raw_string(source, index + 1, hashes).1;
            }
            b'b' if bytes.get(index + 1) == Some(&b'\'') => {
                index = skip_character(bytes, index + 1);
            }
            b'\'' if is_character_literal(bytes, index) => {
                index = skip_character(bytes, index);
            }
            b'r' if bytes.get(index + 1) == Some(&b'#')
                && bytes.get(index + 2).is_some_and(u8::is_ascii_alphabetic) =>
            {
                let start = index + 2;
                index = take_identifier(bytes, start);
                tokens.push(Token::Identifier(&source[start..index]));
            }
            byte if is_identifier_start(byte) => {
                let start = index;
                index = take_identifier(bytes, start);
                tokens.push(Token::Identifier(&source[start..index]));
            }
            byte if byte.is_ascii() => {
                tokens.push(Token::Punctuation(char::from(byte)));
                index += 1;
            }
            _ => {
                index += source[index..]
                    .chars()
                    .next()
                    .expect("index is inside source")
                    .len_utf8();
            }
        }
    }

    tokens
}

fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start + offset + 1)
}

fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
    let mut depth = 1usize;
    let mut index = start;
    while index < bytes.len() && depth > 0 {
        if bytes.get(index..index + 2) == Some(b"/*") {
            depth += 1;
            index += 2;
        } else if bytes.get(index..index + 2) == Some(b"*/") {
            depth -= 1;
            index += 2;
        } else {
            index += 1;
        }
    }
    index
}

fn raw_string_hashes(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'r') {
        return None;
    }
    let mut index = start + 1;
    while bytes.get(index) == Some(&b'#') {
        index += 1;
    }
    (bytes.get(index) == Some(&b'"')).then_some(index - start - 1)
}

fn raw_string(source: &str, start: usize, hashes: usize) -> (&str, usize) {
    let content_start = start + 2 + hashes;
    let closing = format!("\"{}", "#".repeat(hashes));
    let Some(relative_end) = source[content_start..].find(&closing) else {
        return (&source[content_start..], source.len());
    };
    let content_end = content_start + relative_end;
    (
        &source[content_start..content_end],
        content_end + closing.len(),
    )
}

fn quoted_string(source: &str, quote: usize) -> (&str, usize) {
    let bytes = source.as_bytes();
    let content_start = quote + 1;
    let mut index = content_start;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = (index + 2).min(bytes.len()),
            b'"' => return (&source[content_start..index], index + 1),
            _ => index += 1,
        }
    }
    (&source[content_start..], source.len())
}

fn is_character_literal(bytes: &[u8], quote: usize) -> bool {
    match bytes.get(quote + 1) {
        Some(b'\\') => bytes.get(quote + 3) == Some(&b'\''),
        Some(_) => bytes.get(quote + 2) == Some(&b'\''),
        None => false,
    }
}

fn skip_character(bytes: &[u8], quote: usize) -> usize {
    let mut index = quote + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = (index + 2).min(bytes.len()),
            b'\'' => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn take_identifier(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        index += 1;
    }
    index
}
