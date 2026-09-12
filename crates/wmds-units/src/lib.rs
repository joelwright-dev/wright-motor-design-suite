//! Physical quantities with runtime dimensional checking.
//!
//! Every value in WMDS that carries a unit is a [`Quantity`]: an `f64` in SI base units plus a
//! [`Dim`] recording the exponents of each base dimension. Arithmetic checks dimensions, so a
//! length cannot be added to a mass and a force divided by an area is a pressure.
//!
//! Text forms such as `380 mm`, `20 kN`, `1550 kg/m^3` and `0.18 kg*m^2` parse with
//! [`Quantity::parse`]. Currency is modelled as a base dimension so cost expressions are checked
//! like any other.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

use thiserror::Error;

/// Exponents of the base dimensions. Angle and currency are treated as base dimensions because
/// mixing them silently is a common source of design errors.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct Dim {
    pub m: i8,
    pub kg: i8,
    pub s: i8,
    pub a: i8,
    pub k: i8,
    pub rad: i8,
    pub cur: i8,
}

impl Dim {
    pub const NONE: Dim = Dim {
        m: 0,
        kg: 0,
        s: 0,
        a: 0,
        k: 0,
        rad: 0,
        cur: 0,
    };
    pub const LENGTH: Dim = Dim { m: 1, ..Dim::NONE };
    pub const MASS: Dim = Dim { kg: 1, ..Dim::NONE };
    pub const TIME: Dim = Dim { s: 1, ..Dim::NONE };
    pub const ANGLE: Dim = Dim {
        rad: 1,
        ..Dim::NONE
    };
    pub const FORCE: Dim = Dim {
        m: 1,
        kg: 1,
        s: -2,
        ..Dim::NONE
    };
    pub const PRESSURE: Dim = Dim {
        m: -1,
        kg: 1,
        s: -2,
        ..Dim::NONE
    };
    pub const TORQUE: Dim = Dim {
        m: 2,
        kg: 1,
        s: -2,
        ..Dim::NONE
    };
    pub const CURRENCY: Dim = Dim {
        cur: 1,
        ..Dim::NONE
    };

    pub fn is_dimensionless(&self) -> bool {
        *self == Dim::NONE
    }

    fn combine(self, o: Dim, sign: i8) -> Dim {
        Dim {
            m: self.m + sign * o.m,
            kg: self.kg + sign * o.kg,
            s: self.s + sign * o.s,
            a: self.a + sign * o.a,
            k: self.k + sign * o.k,
            rad: self.rad + sign * o.rad,
            cur: self.cur + sign * o.cur,
        }
    }

    fn pow(self, n: i8) -> Dim {
        Dim {
            m: self.m * n,
            kg: self.kg * n,
            s: self.s * n,
            a: self.a * n,
            k: self.k * n,
            rad: self.rad * n,
            cur: self.cur * n,
        }
    }
}

impl fmt::Display for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<(&str, i8)> = [
            ("m", self.m),
            ("kg", self.kg),
            ("s", self.s),
            ("A", self.a),
            ("K", self.k),
            ("rad", self.rad),
            ("AUD", self.cur),
        ]
        .into_iter()
        .filter(|(_, e)| *e != 0)
        .collect();
        if parts.is_empty() {
            return write!(f, "1");
        }
        let mut first = true;
        for (name, e) in parts {
            if !first {
                write!(f, "*")?;
            }
            first = false;
            if e == 1 {
                write!(f, "{name}")?;
            } else {
                write!(f, "{name}^{e}")?;
            }
        }
        Ok(())
    }
}

/// A value in SI base units with its dimension.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Quantity {
    pub value: f64,
    pub dim: Dim,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum UnitError {
    #[error("unknown unit `{0}`")]
    UnknownUnit(String),
    #[error("cannot parse `{0}` as a quantity")]
    Parse(String),
    #[error("dimension mismatch: {0} vs {1}")]
    Mismatch(Dim, Dim),
    #[error("exponent must be an integer in `{0}`")]
    BadExponent(String),
}

/// A named unit: multiply by `factor` to get SI, add `offset` first (temperature scales).
struct UnitDef {
    factor: f64,
    offset: f64,
    dim: Dim,
}

fn unit_table(symbol: &str) -> Option<UnitDef> {
    let d = |factor: f64, dim: Dim| {
        Some(UnitDef {
            factor,
            offset: 0.0,
            dim,
        })
    };
    let m = Dim::LENGTH;
    let kg = Dim::MASS;
    let s = Dim::TIME;
    match symbol {
        // length
        "m" => d(1.0, m),
        "mm" => d(1e-3, m),
        "cm" => d(1e-2, m),
        "km" => d(1e3, m),
        // Spelled `inch`, not `in`: `in` is the list-membership operator in the expression
        // language, and `x in [a, b]` reads better in a compliance rule than inches do in the
        // handful of places the imperial unit is needed (wheel diameters, mostly).
        "inch" => d(0.0254, m),
        // mass
        "kg" => d(1.0, kg),
        "g" => d(1e-3, kg),
        "t" => d(1e3, kg),
        // time
        "s" => d(1.0, s),
        "ms" => d(1e-3, s),
        "min" => d(60.0, s),
        "h" => d(3600.0, s),
        // angle
        "rad" => d(1.0, Dim::ANGLE),
        "deg" => d(std::f64::consts::PI / 180.0, Dim::ANGLE),
        // force, pressure, energy, power
        "N" => d(1.0, Dim::FORCE),
        "kN" => d(1e3, Dim::FORCE),
        "Pa" => d(1.0, Dim::PRESSURE),
        "kPa" => d(1e3, Dim::PRESSURE),
        "MPa" => d(1e6, Dim::PRESSURE),
        "GPa" => d(1e9, Dim::PRESSURE),
        "bar" => d(1e5, Dim::PRESSURE),
        "Nm" => d(1.0, Dim::TORQUE),
        "kNm" => d(1e3, Dim::TORQUE),
        "J" => d(1.0, Dim::TORQUE),
        "kJ" => d(1e3, Dim::TORQUE),
        "kWh" => d(3.6e6, Dim::TORQUE),
        "W" => d(
            1.0,
            Dim {
                m: 2,
                kg: 1,
                s: -3,
                ..Dim::NONE
            },
        ),
        "kW" => d(
            1e3,
            Dim {
                m: 2,
                kg: 1,
                s: -3,
                ..Dim::NONE
            },
        ),
        "Hz" => d(1.0, Dim { s: -1, ..Dim::NONE }),
        "rpm" => d(
            2.0 * std::f64::consts::PI / 60.0,
            Dim {
                s: -1,
                rad: 1,
                ..Dim::NONE
            },
        ),
        // speed
        "kph" => d(
            1000.0 / 3600.0,
            Dim {
                m: 1,
                s: -1,
                ..Dim::NONE
            },
        ),
        "mps" => d(
            1.0,
            Dim {
                m: 1,
                s: -1,
                ..Dim::NONE
            },
        ),
        "g0" => d(
            9.80665,
            Dim {
                m: 1,
                s: -2,
                ..Dim::NONE
            },
        ),
        // electrical
        "A" => d(1.0, Dim { a: 1, ..Dim::NONE }),
        "V" => d(
            1.0,
            Dim {
                m: 2,
                kg: 1,
                s: -3,
                a: -1,
                ..Dim::NONE
            },
        ),
        "Ah" => d(
            3600.0,
            Dim {
                a: 1,
                s: 1,
                ..Dim::NONE
            },
        ),
        // temperature
        "K" => d(1.0, Dim { k: 1, ..Dim::NONE }),
        "C" | "degC" => Some(UnitDef {
            factor: 1.0,
            offset: 273.15,
            dim: Dim { k: 1, ..Dim::NONE },
        }),
        // currency
        "AUD" => d(1.0, Dim::CURRENCY),
        // percent
        "%" => d(0.01, Dim::NONE),
        _ => None,
    }
}

/// Returns true if `s` is a recognised unit symbol.
pub fn is_unit_symbol(s: &str) -> bool {
    unit_table(s).is_some()
}

/// Parse a unit expression such as `mm`, `kg/m^3`, `kg*m^2`, `N/mm`, `AUD/kg`.
/// Returns the SI factor, offset and dimension.
fn parse_unit_expr(text: &str) -> Result<UnitDef, UnitError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(UnitDef {
            factor: 1.0,
            offset: 0.0,
            dim: Dim::NONE,
        });
    }
    let mut factor = 1.0;
    let mut dim = Dim::NONE;
    let mut offset = 0.0;
    let mut sign = 1i8;
    let mut terms = 0;
    // Split on * and / keeping the operators.
    let mut buf = String::new();
    let mut ops: Vec<(i8, String)> = Vec::new();
    for ch in text.chars() {
        match ch {
            '*' | '/' | '·' => {
                ops.push((sign, buf.trim().to_string()));
                buf.clear();
                sign = if ch == '/' { -1 } else { 1 };
            }
            _ => buf.push(ch),
        }
    }
    ops.push((sign, buf.trim().to_string()));
    for (sgn, term) in ops {
        let (sym, exp) = match term.split_once('^') {
            Some((s, e)) => (
                s.trim(),
                e.trim()
                    .parse::<i8>()
                    .map_err(|_| UnitError::BadExponent(text.to_string()))?,
            ),
            None => (term.as_str(), 1),
        };
        let u = unit_table(sym).ok_or_else(|| UnitError::UnknownUnit(sym.to_string()))?;
        let e = exp * sgn;
        factor *= u.factor.powi(e as i32);
        dim = dim.combine(u.dim.pow(exp), sgn);
        if u.offset != 0.0 {
            offset = u.offset;
        }
        terms += 1;
    }
    if terms > 1 {
        offset = 0.0;
    }
    Ok(UnitDef {
        factor,
        offset,
        dim,
    })
}

impl Quantity {
    pub const fn new(value: f64, dim: Dim) -> Quantity {
        Quantity { value, dim }
    }

    pub const fn dimensionless(value: f64) -> Quantity {
        Quantity {
            value,
            dim: Dim::NONE,
        }
    }

    /// Build from a value expressed in `unit`.
    pub fn from_unit(value: f64, unit: &str) -> Result<Quantity, UnitError> {
        let u = parse_unit_expr(unit)?;
        Ok(Quantity {
            value: (value + u.offset) * u.factor,
            dim: u.dim,
        })
    }

    /// Parse `"380 mm"`, `"380mm"`, `"-2.5e3 N"`, `"1550 kg/m^3"`, `"12"`.
    pub fn parse(text: &str) -> Result<Quantity, UnitError> {
        let t = text.trim();
        let split = t
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
            .map(|(i, _)| i)
            .unwrap_or(t.len());
        // Guard against "e" being consumed as part of a unit like "e355" (not a unit anyway) or
        // a number like "1e3": try the longest numeric prefix that parses.
        let mut num_end = split;
        let mut number: Option<f64> = None;
        while num_end > 0 {
            if let Ok(v) = t[..num_end].trim().parse::<f64>() {
                number = Some(v);
                break;
            }
            num_end -= 1;
        }
        let number = number.ok_or_else(|| UnitError::Parse(text.to_string()))?;
        let unit = t[num_end..].trim();
        Quantity::from_unit(number, unit)
    }

    /// Value expressed in `unit`.
    pub fn to_unit(&self, unit: &str) -> Result<f64, UnitError> {
        let u = parse_unit_expr(unit)?;
        if u.dim != self.dim {
            return Err(UnitError::Mismatch(self.dim, u.dim));
        }
        Ok(self.value / u.factor - u.offset)
    }

    /// Dimension of the unit expression, for validating declared parameter units.
    pub fn dim_of_unit(unit: &str) -> Result<Dim, UnitError> {
        Ok(parse_unit_expr(unit)?.dim)
    }

    pub fn same_dim(&self, other: &Quantity) -> Result<(), UnitError> {
        if self.dim == other.dim {
            Ok(())
        } else {
            Err(UnitError::Mismatch(self.dim, other.dim))
        }
    }

    pub fn try_add(self, o: Quantity) -> Result<Quantity, UnitError> {
        self.same_dim(&o)?;
        Ok(Quantity {
            value: self.value + o.value,
            dim: self.dim,
        })
    }

    pub fn try_sub(self, o: Quantity) -> Result<Quantity, UnitError> {
        self.same_dim(&o)?;
        Ok(Quantity {
            value: self.value - o.value,
            dim: self.dim,
        })
    }

    pub fn powi(self, n: i8) -> Quantity {
        Quantity {
            value: self.value.powi(n as i32),
            dim: self.dim.pow(n),
        }
    }

    pub fn sqrt(self) -> Result<Quantity, UnitError> {
        let d = self.dim;
        let all_even = [d.m, d.kg, d.s, d.a, d.k, d.rad, d.cur]
            .iter()
            .all(|e| e % 2 == 0);
        if !all_even {
            return Err(UnitError::Mismatch(d, d));
        }
        Ok(Quantity {
            value: self.value.sqrt(),
            dim: Dim {
                m: d.m / 2,
                kg: d.kg / 2,
                s: d.s / 2,
                a: d.a / 2,
                k: d.k / 2,
                rad: d.rad / 2,
                cur: d.cur / 2,
            },
        })
    }

    pub fn abs(self) -> Quantity {
        Quantity {
            value: self.value.abs(),
            dim: self.dim,
        }
    }

    /// Preferred display unit for a dimension, used by reports and the CLI.
    pub fn preferred_unit(&self) -> &'static str {
        let d = self.dim;
        if d == Dim::NONE {
            ""
        } else if d == Dim::LENGTH {
            "mm"
        } else if d == Dim::MASS {
            "kg"
        } else if d == Dim::TIME {
            "s"
        } else if d == Dim::ANGLE {
            "deg"
        } else if d == Dim::FORCE {
            "N"
        } else if d == Dim::PRESSURE {
            "MPa"
        } else if d == Dim::TORQUE {
            "Nm"
        } else if d == Dim::CURRENCY {
            "AUD"
        } else if d
            == (Dim {
                m: -3,
                kg: 1,
                ..Dim::NONE
            })
        {
            "kg/m^3"
        } else if d
            == (Dim {
                m: 2,
                kg: 1,
                ..Dim::NONE
            })
        {
            "kg*m^2"
        } else if d == (Dim { m: 2, ..Dim::NONE }) {
            "mm^2"
        } else if d == (Dim { m: 3, ..Dim::NONE }) {
            "mm^3"
        } else if d
            == (Dim {
                kg: 1,
                s: -2,
                ..Dim::NONE
            })
        {
            "N/mm"
        } else {
            "SI"
        }
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = self.preferred_unit();
        if unit.is_empty() {
            write!(f, "{}", trim_float(self.value))
        } else if unit == "SI" {
            write!(f, "{} {}", trim_float(self.value), self.dim)
        } else {
            let v = self.to_unit(unit).unwrap_or(self.value);
            write!(f, "{} {}", trim_float(v), unit)
        }
    }
}

fn trim_float(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e12 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.6}", v);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        s.to_string()
    }
}

impl Add for Quantity {
    type Output = Quantity;
    /// Panics on dimension mismatch; use `try_add` where the dimensions are not known statically.
    fn add(self, o: Quantity) -> Quantity {
        self.try_add(o).expect("dimension mismatch in add")
    }
}
impl Sub for Quantity {
    type Output = Quantity;
    fn sub(self, o: Quantity) -> Quantity {
        self.try_sub(o).expect("dimension mismatch in sub")
    }
}
impl Mul for Quantity {
    type Output = Quantity;
    fn mul(self, o: Quantity) -> Quantity {
        Quantity {
            value: self.value * o.value,
            dim: self.dim.combine(o.dim, 1),
        }
    }
}
impl Div for Quantity {
    type Output = Quantity;
    fn div(self, o: Quantity) -> Quantity {
        Quantity {
            value: self.value / o.value,
            dim: self.dim.combine(o.dim, -1),
        }
    }
}
impl Mul<f64> for Quantity {
    type Output = Quantity;
    fn mul(self, k: f64) -> Quantity {
        Quantity {
            value: self.value * k,
            dim: self.dim,
        }
    }
}
impl Neg for Quantity {
    type Output = Quantity;
    fn neg(self) -> Quantity {
        Quantity {
            value: -self.value,
            dim: self.dim,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * (1.0 + a.abs().max(b.abs()))
    }

    #[test]
    fn parses_simple_units() {
        let q = Quantity::parse("380 mm").unwrap();
        assert_eq!(q.dim, Dim::LENGTH);
        assert!(approx(q.value, 0.380));
        assert!(approx(Quantity::parse("380mm").unwrap().value, 0.380));
        assert!(approx(Quantity::parse("20 kN").unwrap().value, 20_000.0));
        assert!(approx(
            Quantity::parse("12 deg").unwrap().value,
            12.0_f64.to_radians()
        ));
        assert!(approx(Quantity::parse("-2.5e3 N").unwrap().value, -2500.0));
        assert_eq!(
            Quantity::parse("12").unwrap(),
            Quantity::dimensionless(12.0)
        );
    }

    #[test]
    fn parses_compound_units() {
        let rho = Quantity::parse("1550 kg/m^3").unwrap();
        assert_eq!(
            rho.dim,
            Dim {
                m: -3,
                kg: 1,
                ..Dim::NONE
            }
        );
        let i = Quantity::parse("0.18 kg*m^2").unwrap();
        assert_eq!(
            i.dim,
            Dim {
                m: 2,
                kg: 1,
                ..Dim::NONE
            }
        );
        let k = Quantity::parse("25 N/mm").unwrap();
        assert!(approx(k.value, 25_000.0));
        let c = Quantity::parse("0.9 AUD/mm").unwrap();
        assert_eq!(
            c.dim,
            Dim {
                m: -1,
                cur: 1,
                ..Dim::NONE
            }
        );
        let t = Quantity::parse("120 C").unwrap();
        assert!(approx(t.value, 393.15));
    }

    #[test]
    fn arithmetic_checks_dimensions() {
        let a = Quantity::parse("100 mm").unwrap();
        let b = Quantity::parse("2 kg").unwrap();
        assert!(a.try_add(b).is_err());
        let f = Quantity::parse("10 N").unwrap();
        let area = Quantity::parse("2 mm").unwrap() * Quantity::parse("5 mm").unwrap();
        let p = f / area;
        assert_eq!(p.dim, Dim::PRESSURE);
        assert!(approx(p.to_unit("MPa").unwrap(), 1.0));
    }

    #[test]
    fn display_uses_preferred_units() {
        assert_eq!(Quantity::parse("0.38 m").unwrap().to_string(), "380 mm");
        assert_eq!(Quantity::parse("20 kN").unwrap().to_string(), "20000 N");
        assert_eq!(Quantity::dimensionless(2.5).to_string(), "2.5");
    }

    #[test]
    fn rejects_unknown_units() {
        assert!(matches!(
            Quantity::parse("3 furlongs"),
            Err(UnitError::UnknownUnit(_))
        ));
    }
}
