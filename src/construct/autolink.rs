//! Autolinks are a construct that occurs in the [text][] content type.
//!
//! It forms with the following BNF:
//!
//! ```bnf
//! autolink ::= '<' ( url | email ) '>'
//!
//! url ::= ascii_alphabetic 0*31( '+' '-' '.' ascii_alphanumeric ) ':' *( code - ascii_control - '\r' - '\n' - ' ')
//! email ::= 1*ascii_atext '@' domain *('.' domain)
//! ; Restriction: up to (including) 63 character are allowed in each domain.
//! domain ::= ascii_alphanumeric *( ascii_alphanumeric | '-' ascii_alphanumeric )
//! ascii_atext ::= ascii_alphanumeric | '#' .. '\'' | '*' | '+' | '-' | '/' | '=' | '?' | '^' .. '`' | '{' .. '~'
//! ```
//!
//! Autolinks relate to the `<a>` element in HTML.
//! See [*§ 4.5.1 The `a` element*][html-a] in the HTML spec for more info.
//! When an email autolink is used (so, without a protocol), the string
//! `mailto:` is prepended before the email, when generating the `href`
//! attribute of the hyperlink.
//!
//! The maximum allowed size of a scheme is `31` (inclusive), which is defined
//! in [`AUTOLINK_SCHEME_SIZE_MAX`][autolink_scheme_size_max].
//! The maximum allowed size of a domain is `63` (inclusive), which is defined
//! in [`AUTOLINK_DOMAIN_SIZE_MAX`][autolink_domain_size_max].
//!
//! The grammar for autolinks is quite strict and prohibits the use of ASCII control
//! characters or spaces.
//! To use non-ascii characters and otherwise impossible characters, in URLs,
//! you can use percent encoding:
//!
//! ```markdown
//! <https://example.com/alpha%20bravo>
//! ```
//!
//! Yields:
//!
//! ```html
//! <p><a href="https://example.com/alpha%20bravo">https://example.com/alpha%20bravo</a></p>
//! ```
//!
//! There are several cases where incorrect encoding of URLs would, in other
//! languages, result in a parse error.
//! In markdown, there are no errors, and URLs are normalized.
//! In addition, unicode characters are percent encoded
//! ([`sanitize_uri`][sanitize_uri]).
//! For example:
//!
//! ```markdown
//! <https://a👍b%>
//! ```
//!
//! Yields:
//!
//! ```html
//! <p><a href="https://a%F0%9F%91%8Db%25">https://a👍b%</a></p>
//! ```
//!
//! Interestingly, there are a couple of things that are valid autolinks in
//! markdown but in HTML would be valid tags, such as `<svg:rect>` and
//! `<xml:lang/>`.
//! However, because `CommonMark` employs a naïve HTML parsing algorithm, those
//! are not considered HTML.
//!
//! While `CommonMark` restricts links from occurring in other links in the
//! case of labels (see [label end][label_end]), this restriction is not in
//! place for autolinks inside labels:
//!
//! ```markdown
//! [<https://example.com>](#)
//! ```
//!
//! Yields:
//!
//! ```html
//! <p><a href="#"><a href="https://example.com">https://example.com</a></a></p>
//! ```
//!
//! The generated output, in this case, is invalid according to HTML.
//! When a browser sees that markup, it will instead parse it as:
//!
//! ```html
//! <p><a href="#"></a><a href="https://example.com">https://example.com</a></p>
//! ```
//!
//! ## Tokens
//!
//! *   [`Autolink`][Token::Autolink]
//! *   [`AutolinkEmail`][Token::AutolinkEmail]
//! *   [`AutolinkMarker`][Token::AutolinkMarker]
//! *   [`AutolinkProtocol`][Token::AutolinkProtocol]
//!
//! ## References
//!
//! *   [`autolink.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-core-commonmark/dev/lib/autolink.js)
//! *   [*§ 6.4 Autolinks* in `CommonMark`](https://spec.commonmark.org/0.30/#autolinks)
//!
//! [text]: crate::content::text
//! [label_end]: crate::construct::label_end
//! [autolink_scheme_size_max]: crate::constant::AUTOLINK_SCHEME_SIZE_MAX
//! [autolink_domain_size_max]: crate::constant::AUTOLINK_DOMAIN_SIZE_MAX
//! [sanitize_uri]: crate::util::sanitize_uri
//! [html-a]: https://html.spec.whatwg.org/multipage/text-level-semantics.html#the-a-element

use crate::constant::{AUTOLINK_DOMAIN_SIZE_MAX, AUTOLINK_SCHEME_SIZE_MAX};
use crate::token::Token;
use crate::tokenizer::{State, StateName, Tokenizer};

/// Start of an autolink.
///
/// ```markdown
/// > | a<https://example.com>b
///      ^
/// > | a<user@example.com>b
///      ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'<') if tokenizer.parse_state.constructs.autolink => {
            tokenizer.enter(Token::Autolink);
            tokenizer.enter(Token::AutolinkMarker);
            tokenizer.consume();
            tokenizer.exit(Token::AutolinkMarker);
            tokenizer.enter(Token::AutolinkProtocol);
            State::Next(StateName::AutolinkOpen)
        }
        _ => State::Nok,
    }
}

/// After `<`, before the protocol.
///
/// ```markdown
/// > | a<https://example.com>b
///       ^
/// > | a<user@example.com>b
///       ^
/// ```
pub fn open(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII alphabetic.
        Some(b'A'..=b'Z' | b'a'..=b'z') => {
            tokenizer.consume();
            State::Next(StateName::AutolinkSchemeOrEmailAtext)
        }
        _ => State::Retry(StateName::AutolinkEmailAtext),
    }
}

/// After the first byte of the protocol or email name.
///
/// ```markdown
/// > | a<https://example.com>b
///        ^
/// > | a<user@example.com>b
///        ^
/// ```
pub fn scheme_or_email_atext(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII alphanumeric and `+`, `-`, and `.`.
        Some(b'+' | b'-' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z') => {
            // Count the previous alphabetical from `open` too.
            tokenizer.tokenize_state.size = 1;
            State::Retry(StateName::AutolinkSchemeInsideOrEmailAtext)
        }
        _ => State::Retry(StateName::AutolinkEmailAtext),
    }
}

/// Inside an ambiguous protocol or email name.
///
/// ```markdown
/// > | a<https://example.com>b
///        ^
/// > | a<user@example.com>b
///        ^
/// ```
pub fn scheme_inside_or_email_atext(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b':') => {
            tokenizer.consume();
            tokenizer.tokenize_state.size = 0;
            State::Next(StateName::AutolinkUrlInside)
        }
        // ASCII alphanumeric and `+`, `-`, and `.`.
        Some(b'+' | b'-' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z')
            if tokenizer.tokenize_state.size < AUTOLINK_SCHEME_SIZE_MAX =>
        {
            tokenizer.tokenize_state.size += 1;
            tokenizer.consume();
            State::Next(StateName::AutolinkSchemeInsideOrEmailAtext)
        }
        _ => {
            tokenizer.tokenize_state.size = 0;
            State::Retry(StateName::AutolinkEmailAtext)
        }
    }
}

/// Inside a URL, after the protocol.
///
/// ```markdown
/// > | a<https://example.com>b
///             ^
/// ```
pub fn url_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'>') => {
            tokenizer.exit(Token::AutolinkProtocol);
            tokenizer.enter(Token::AutolinkMarker);
            tokenizer.consume();
            tokenizer.exit(Token::AutolinkMarker);
            tokenizer.exit(Token::Autolink);
            State::Ok
        }
        // ASCII control, space, or `<`.
        None | Some(b'\0'..=0x1F | b' ' | b'<' | 0x7F) => State::Nok,
        Some(_) => {
            tokenizer.consume();
            State::Next(StateName::AutolinkUrlInside)
        }
    }
}

/// Inside email atext.
///
/// ```markdown
/// > | a<user.name@example.com>b
///              ^
/// ```
pub fn email_atext(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'@') => {
            tokenizer.consume();
            State::Next(StateName::AutolinkEmailAtSignOrDot)
        }
        // ASCII atext.
        //
        // atext is an ASCII alphanumeric (see [`is_ascii_alphanumeric`][]), or
        // a byte in the inclusive ranges U+0023 NUMBER SIGN (`#`) to U+0027
        // APOSTROPHE (`'`), U+002A ASTERISK (`*`), U+002B PLUS SIGN (`+`),
        // U+002D DASH (`-`), U+002F SLASH (`/`), U+003D EQUALS TO (`=`),
        // U+003F QUESTION MARK (`?`), U+005E CARET (`^`) to U+0060 GRAVE
        // ACCENT (`` ` ``), or U+007B LEFT CURLY BRACE (`{`) to U+007E TILDE
        // (`~`).
        //
        // See:
        // **\[RFC5322]**:
        // [Internet Message Format](https://tools.ietf.org/html/rfc5322).
        // P. Resnick.
        // IETF.
        //
        // [`is_ascii_alphanumeric`]: char::is_ascii_alphanumeric
        Some(
            b'#'..=b'\'' | b'*' | b'+' | b'-'..=b'9' | b'=' | b'?' | b'A'..=b'Z' | b'^'..=b'~',
        ) => {
            tokenizer.consume();
            State::Next(StateName::AutolinkEmailAtext)
        }
        _ => State::Nok,
    }
}

/// After an at-sign or a dot in the label.
///
/// ```markdown
/// > | a<user.name@example.com>b
///                 ^       ^
/// ```
pub fn email_at_sign_or_dot(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII alphanumeric.
        Some(b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z') => {
            State::Retry(StateName::AutolinkEmailValue)
        }
        _ => State::Nok,
    }
}

/// In the label, where `.` and `>` are allowed.
///
/// ```markdown
/// > | a<user.name@example.com>b
///                   ^
/// ```
pub fn email_label(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'.') => {
            tokenizer.tokenize_state.size = 0;
            tokenizer.consume();
            State::Next(StateName::AutolinkEmailAtSignOrDot)
        }
        Some(b'>') => {
            tokenizer.tokenize_state.size = 0;
            let index = tokenizer.events.len();
            tokenizer.exit(Token::AutolinkProtocol);
            // Change the token type.
            tokenizer.events[index - 1].token_type = Token::AutolinkEmail;
            tokenizer.events[index].token_type = Token::AutolinkEmail;
            tokenizer.enter(Token::AutolinkMarker);
            tokenizer.consume();
            tokenizer.exit(Token::AutolinkMarker);
            tokenizer.exit(Token::Autolink);
            State::Ok
        }
        _ => State::Retry(StateName::AutolinkEmailValue),
    }
}

/// In the label, where `.` and `>` are *not* allowed.
///
/// Though, this is also used in `email_label` to parse other values.
///
/// ```markdown
/// > | a<user.name@ex-ample.com>b
///                    ^
/// ```
pub fn email_value(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // ASCII alphanumeric or `-`.
        Some(b'-' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z')
            if tokenizer.tokenize_state.size < AUTOLINK_DOMAIN_SIZE_MAX =>
        {
            let name = if matches!(tokenizer.current, Some(b'-')) {
                StateName::AutolinkEmailValue
            } else {
                StateName::AutolinkEmailLabel
            };
            tokenizer.tokenize_state.size += 1;
            tokenizer.consume();
            State::Next(name)
        }
        _ => {
            tokenizer.tokenize_state.size = 0;
            State::Nok
        }
    }
}
