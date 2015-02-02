// DOCS

#![feature(plugin,core,std_misc,collections)]
#[plugin] #[no_link] extern crate peg_syntax_ext;

use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::str::FromStr;


pub struct Property {
    /// Parameters.
    pub params: HashMap<String, String>,

    /// Value as unparsed string.
    pub raw_value: String,

    /// Property group. E.g. a contentline like `foo.FN:Markus` would result in the group being
    /// `"foo"`.
    pub prop_group: Option<String>
}

impl Property {
    /// Create property from unescaped string.
    pub fn new(value: &str) -> Property {
        Property {
            params: HashMap::new(),
            raw_value: escape_chars(value),
            prop_group: None
        }
    }

    /// Get value as unescaped string.
    pub fn value_as_string(&self) -> String {
        unescape_chars(self.raw_value.as_slice())
    }
}


pub struct Component {
    /// The name of the component, such as `VCARD` or `VEVENT`.
    pub name: String,

    /// The component's properties.
    pub props: HashMap<String, Vec<Property>>,

    /// The component's child- or sub-components.
    pub subcomponents: Vec<Component>
}

impl Component {
    pub fn new(name: &str) -> Component {
        Component {
            name: name.to_string(),
            props: HashMap::new(),
            subcomponents: vec![]
        }
    }

    /// Retrieve one property (from many) by key. Returns `None` if nothing is found.
    pub fn single_prop(&self, key: &str) -> Option<&Property> {
        match self.props.get(key) {
            Some(x) => {
                match x.len() {
                    1 => Some(&x[0]),
                    _ => None
                }
            },
            None => None
        }
    }

    /// Retrieve a mutable vector of properties for this key. Creates one (and inserts it into the
    /// component) if none exists.
    pub fn all_props_mut(&mut self, key: &str) -> &mut Vec<Property> {
        match self.props.entry(String::from_str(key)) {
            Occupied(values) => values.into_mut(),
            Vacant(values) => values.insert(vec![])
        }
    }

    /// Retrieve properties by key. Returns an empty slice if key doesn't exist.
    pub fn all_props(&self, key: &str) -> &[Property] {
        static EMPTY: &'static [Property] = &[];
        match self.props.get(key) {
            Some(values) => values.as_slice(),
            None => EMPTY
        }
    }
}

impl FromStr for Component {
    /// Same as `vobject::parse_component`, but without the error messages.
    fn from_str(s: &str) -> Option<Component> {
        match parse_component(s) {
            Ok(x) => Some(x),
            Err(_) => None
        }
    }
}

/// Parse a component. The error value is a human-readable message.
pub fn parse_component(s: &str) -> Result<Component, String> {
    // XXX: The unfolding should be worked into the PEG
    // See feature request: https://github.com/kevinmehall/rust-peg/issues/26
    let unfolded = unfold_lines(s);
    parser::component(unfolded.as_slice())
}

/// Write a component. The error value is a human-readable message.
pub fn write_component(c: &Component) -> String {
    fn inner(buf: &mut String, c: &Component) {
        buf.push_str("BEGIN:");
        buf.push_str(c.name.as_slice());
        buf.push_str("\r\n");

        for (prop_name, props) in c.props.iter() {
            for prop in props.iter() {
                match prop.prop_group {
                    Some(ref x) => { buf.push_str(x.as_slice()); buf.push('.'); },
                    None => ()
                };
                buf.push_str(prop_name.as_slice());
                for (param_key, param_value) in prop.params.iter() {
                    buf.push(';');
                    buf.push_str(param_key.as_slice());
                    buf.push('=');
                    buf.push_str(param_value.as_slice());
                };
                buf.push(':');
                buf.push_str(fold_line(prop.raw_value.as_slice()).as_slice());
                buf.push_str("\r\n");
            };
        };

        for subcomponent in c.subcomponents.iter() {
            inner(buf, subcomponent);
        };

        buf.push_str("END:");
        buf.push_str(c.name.as_slice());
        buf.push_str("\r\n");
    }

    let mut buf = String::new();
    inner(&mut buf, c);
    buf
}

/// Escape text for a VObject property value.
pub fn escape_chars(s: &str) -> String {
    // Order matters! Lifted from icalendar.parser
    // https://github.com/collective/icalendar/
    s
        .replace("\\N", "\n")
        .replace("\\", "\\\\")
        .replace(";", "\\;")
        .replace(",", "\\,")
        .replace("\r\n", "\\n")
        .replace("\n", "\\n")
}

/// Unescape text from a VObject property value.
pub fn unescape_chars(s: &str) -> String {
    // Order matters! Lifted from icalendar.parser
    // https://github.com/collective/icalendar/
    s
        .replace("\\N", "\\n")
        .replace("\r\n", "\n")
        .replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// Unfold contentline.
pub fn unfold_lines(s: &str) -> String {
    s
        .replace("\r\n ", "").replace("\r\n\t", "")
        .replace("\n ", "").replace("\n\t", "")
        .replace("\r ", "").replace("\r\t", "")
}

/// Fold contentline to 75 chars. This function assumes the input to be unfolded, which means no
/// '\n' or '\r' in it.
pub fn fold_line(s: &str) -> String {
    let mut rv = String::new();
    for (i, c) in s.chars().enumerate() {
        rv.push(c);
        if i != 0 && i % 75 == 0 {
            rv.push_str("\r\n ");
        };
    };
    rv
}


peg! parser(r#"
use super::{Component,Property};
use std::collections::HashMap;

components -> Vec<Component>
    = cs:component ** eols __ { cs }

    #[pub]
    component -> Component
        = name:component_begin
          ps:props
          cs:components
          component_end {
            let mut rv = Component::new(name);
            rv.subcomponents = cs;

            for (k, v) in ps.into_iter() {
                rv.all_props_mut(k).push(v);
            };

            rv
        }

    component_begin -> &'input str
        = "BEGIN:" v:value __ { v }

    component_end -> &'input str
        = "END:" v:value __ { v }

props -> Vec<(&'input str, Property)>
    = ps:prop ++ eols __ { ps }

    prop -> (&'input str, Property)
        = !"BEGIN:" !"END:" g:group? k:name p:params ":" v:value {
            (k, Property { params: p, raw_value: v.to_string(), prop_group: g })
        }

    group -> String
        = g:group_name "." { g.to_string() }

        group_name -> &'input str
            = group_char+ { match_str }

    name -> &'input str
        = iana_token+ { match_str }

    params -> HashMap<String, String>
        = ps:(";" p:param {p})* {
            let mut rv: HashMap<String, String> = HashMap::with_capacity(ps.len());
            rv.extend(ps.into_iter().map(|(k, v)| (k.to_string(), v.to_string())));
            rv
        }

        param -> (&'input str, &'input str)
            // FIXME: Doesn't handle comma-separated values
            = k:param_name v:("=" v:param_value { v })? {
                (k, match v {
                    Some(x) => x,
                    None => ""
                })
            }

        param_name -> &'input str
            = iana_token+ { match_str }

        param_value -> &'input str
            = x:(quoted_string / param_text) { x }

        param_text -> &'input str
            = safe_char* { match_str }

    value -> &'input str
        = value_char+ { match_str }


quoted_string -> &'input str
    = dquote x:quoted_content dquote { x }

quoted_content -> &'input str
    = qsafe_char* { match_str }

iana_token = ([a-zA-Z0-9] / "-")+
group_char = ([a-zA-Z0-9] / "-")
qsafe_char = !dquote !ctl value_char
safe_char = !";" !":" qsafe_char

value_char = !eol .

eol = "\r\n" / "\n" / "\r"
dquote = "\""
eols = eol+

// Taken from vCard. vCalendar's is a subset. Together with the definition of "qsafe_char" this
// might reject a bunch of valid iCalendars, but I can't imagine one.
ctl = [\u{00}-\u{1F}] / "\u{7F}"

whitespace = " " / "\t"
__ = (eol / whitespace)*

"#);
