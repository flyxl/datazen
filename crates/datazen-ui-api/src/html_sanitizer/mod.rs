mod parse;
mod sanitize;
mod types;

pub use parse::{parse_html, serialize_html};
pub use sanitize::{regex_sanitize, sanitize_html};
pub use types::{
    HtmlNode, STRIP_TAGS, VOID_TAGS, decode_entities, get_attr, is_event_attr, is_javascript_href,
    is_strip_tag, is_void_tag, set_attr,
};
