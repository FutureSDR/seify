use nom::bytes::complete::tag;
use nom::bytes::complete::take_while1;
use nom::character::complete::multispace0;
use nom::error::{FromExternalError, ParseError};
use nom::multi::separated_list0;
use nom::sequence::delimited;
use nom::sequence::separated_pair;
use nom::IResult;
use nom::branch::alt;
use nom::character::complete::none_of;
use nom::bytes::complete::escaped;
use std::collections::HashMap;
use std::str::FromStr;

use crate::Error;

/// Configuration.
#[derive(Debug)]
pub struct Config {
    map: HashMap<String, String>,
}

impl Config {
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

    delimited(multispace0, 
        alt((
        delimited(tag("'"), esc_or_empty_single, tag("'")),
        delimited(tag("\""), esc_or_empty_double, tag("\"")),
        take_while1(filter))),
        multispace0)(input)
}

impl FromStr for Config {
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
        Ok(Config {
            map: HashMap::from_iter(v.1.iter().cloned().map(|(a, b)| (a.into(), b.into()))),
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deserialize_empty() {
        let c: Config = "".parse().unwrap();
        assert_eq!(c.map.len(), 0);
    }
    #[test]
    fn deserialize_single() {
        let c: Config = "foo=bar".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.map.len(), 1);
    }
    #[test]
    fn deserialize_more() {
        let c: Config = "foo=bar,fo=ba".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba");
        assert_eq!(c.map.len(), 2);
    }
    #[test]
    fn deserialize_whitespace() {
        let c: Config = "   foo  = bar  ,     fo=ba    ".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba");
        assert_eq!(c.map.len(), 2);
    }
    #[test]
    fn deserialize_nonascii() {
        let c: Config = "   f-oo  = b_ar".parse().unwrap();
        assert_eq!(c.get::<String>("f-oo").unwrap(), "b_ar");
        assert_eq!(c.map.len(), 1);
    }
    #[test]
    fn deserialize_dquoted() {
        let c: Config = "foo=bar,fo=\"ba ,\"".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba ,");
        assert_eq!(c.map.len(), 2);
    }
    #[test]
    fn deserialize_squoted() {
        let c: Config = "foo=bar,fo='ba ,\"', hello   ='a s d f '".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba ,\"");
        assert_eq!(c.get::<String>("hello").unwrap(), "a s d f ");
        assert_eq!(c.map.len(), 3);
    }
    #[test]
    fn config_get() {
        let c: Config = "foo=123,bar=lol".parse().unwrap();
        assert_eq!(c.map.len(), 2);
        assert_eq!(c.get::<u32>("foo").unwrap(), 123);
        assert_eq!(c.get::<String>("foo").unwrap(), "123");
        assert_eq!(c.get::<String>("fooo"), Err(Error::NotFound));
        assert_eq!(c.get::<String>("bar").unwrap(), "lol");
        assert_eq!(c.get::<u32>("bar"), Err(Error::ValueError));
    }
}
