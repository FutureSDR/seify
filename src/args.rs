use nom::branch::alt;
use nom::bytes::complete::escaped;
use nom::bytes::complete::tag;
use nom::bytes::complete::take_while1;
use nom::character::complete::multispace0;
use nom::character::complete::none_of;
use nom::error::{FromExternalError, ParseError};
use nom::multi::separated_list0;
use nom::sequence::delimited;
use nom::sequence::separated_pair;
use nom::IResult;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
use std::collections::HashMap;
use std::str::FromStr;

use crate::Error;

/// Arguments.
#[derive(Clone, Serialize)]
#[serde(transparent)]
#[serde_as]
pub struct Args {
    #[serde_with(as = "BTreeMap<_, Vec<DisplayFromStr>>")]
    map: HashMap<String, String>,
}

impl Args {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
    pub fn get<V: FromStr<Err = impl std::error::Error>>(
        &self,
        v: impl AsRef<str>,
    ) -> Result<V, Error> {
        self.map
            .get(v.as_ref())
            .ok_or(Error::NotFound)
            .and_then(|v| v.parse().or(Err(Error::ValueError)))
    }
    pub fn set<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) -> Option<String> {
        self.map.insert(key.into(), value.into())
    }
    pub fn iter<'a>(&'a self) -> std::collections::hash_map::Iter<'a, String, String> {
        self.map.iter()
    }
    pub fn iter_mut<'a>(&'a mut self) -> std::collections::hash_map::IterMut<'a, String, String> {
        self.map.iter_mut()
    }
    pub fn map(&self) -> &HashMap<String, String> {
        &self.map
    }
    pub fn deserialize<D: for<'a> Deserialize<'a>>(&self) -> Option<D> {
        let s = serde_json::to_string(&self).ok()?;
        serde_json::from_str(&s).ok()
    }
}

impl std::fmt::Debug for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.map.fmt(f)
    }
}

impl std::fmt::Display for Args {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut i = self.iter();
        if let Some((k, v)) = i.next() {
            write!(fmt, "{}={}", k, v)?;
            while let Some((k, v)) = i.next() {
                write!(fmt, ", {}={}", k, v)?;
            }
        }
        Ok(())
    }
}

fn parse_string<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + FromExternalError<&'a str, std::num::ParseIntError> + std::fmt::Debug,
{
    let esc_single = escaped(none_of("\\\'"), '\\', tag("'"));
    let esc_or_empty_single = alt((esc_single, tag("")));
    let esc_double = escaped(none_of("\\\""), '\\', tag("\""));
    let esc_or_empty_double = alt((esc_double, tag("")));
    let filter = |c: char| c != ',' && c != '=' && !c.is_whitespace();

    delimited(
        multispace0,
        alt((
            delimited(tag("'"), esc_or_empty_single, tag("'")),
            delimited(tag("\""), esc_or_empty_double, tag("\"")),
            take_while1(filter),
        )),
        multispace0,
    )(input)
}

impl FromStr for Args {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = separated_list0(
            delimited(multispace0, tag(","), multispace0),
            separated_pair(
                parse_string::<nom::error::Error<_>>,
                delimited(multispace0, tag("="), multispace0),
                parse_string,
            ),
        )(s)
        .or(Err(Error::ValueError))?;
        Ok(Args {
            map: HashMap::from_iter(v.1.iter().cloned().map(|(a, b)| (a.into(), b.into()))),
        })
    }
}

// impl for T: AsRef<str> once this issue is resolved
// https://github.com/rust-lang/rust/issues/50133#issuecomment-64690839
impl TryInto<Args> for &str {
    type Error = Error;

    fn try_into(self) -> Result<Args, Self::Error> {
        self.parse()
    }
}

impl TryInto<Args> for String {
    type Error = Error;

    fn try_into(self) -> Result<Args, Self::Error> {
        self.parse()
    }
}

impl TryInto<Args> for &String {
    type Error = Error;

    fn try_into(self) -> Result<Args, Self::Error> {
        self.parse()
    }
}

impl From<&Args> for Args {
    fn from(value: &Args) -> Self {
        value.clone()
    }
}

impl From<()> for Args {
    fn from(_value: ()) -> Self {
        Args::new()
    }
}

impl Default for Args {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_empty() {
        let c: Args = "".parse().unwrap();
        assert_eq!(c.map.len(), 0);
    }
    #[test]
    fn deserialize_single() {
        let c: Args = "foo=bar".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.map.len(), 1);
    }
    #[test]
    fn deserialize_more() {
        let c: Args = "foo=bar,fo=ba".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba");
        assert_eq!(c.map.len(), 2);
    }
    #[test]
    fn deserialize_whitespace() {
        let c: Args = "   foo  = bar  ,     fo=ba    ".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba");
        assert_eq!(c.map.len(), 2);
    }
    #[test]
    fn deserialize_nonascii() {
        let c: Args = "   f-oo  = b_ar".parse().unwrap();
        assert_eq!(c.get::<String>("f-oo").unwrap(), "b_ar");
        assert_eq!(c.map.len(), 1);
    }
    #[test]
    fn deserialize_dquoted() {
        let c: Args = "foo=bar,fo=\"ba ,\"".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba ,");
        assert_eq!(c.map.len(), 2);
    }
    #[test]
    fn deserialize_squoted() {
        let c: Args = "foo=bar,fo='ba ,\"', hello   ='a s d f '".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba ,\"");
        assert_eq!(c.get::<String>("hello").unwrap(), "a s d f ");
        assert_eq!(c.map.len(), 3);
    }
    #[test]
    fn config_get() {
        let c: Args = "foo=123,bar=lol".parse().unwrap();
        assert_eq!(c.map.len(), 2);
        assert_eq!(c.get::<u32>("foo").unwrap(), 123);
        assert_eq!(c.get::<String>("foo").unwrap(), "123");
        assert_eq!(c.get::<String>("fooo"), Err(Error::NotFound));
        assert_eq!(c.get::<String>("bar").unwrap(), "lol");
        assert_eq!(c.get::<u32>("bar"), Err(Error::ValueError));
    }
    #[test]
    fn serde() {
        use serde::Deserialize;
        use serde_with::serde_as;
        use serde_with::DisplayFromStr;

        #[serde_as]
        #[derive(Deserialize)]
        struct Foo {
            #[serde_as(as = "DisplayFromStr")]
            bar: u32,
        }

        let c: Args = "bar=123,hello=world".parse().unwrap();
        let s = serde_json::to_string(&c).unwrap();
        let f: Foo = serde_json::from_str(&s).unwrap();
        assert_eq!(f.bar, 123);
    }
}
