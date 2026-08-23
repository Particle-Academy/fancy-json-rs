//! The value tree: [`Value`], [`Number`] and [`Map`].

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// A JSON number.
///
/// Three variants rather than one `f64`, and that is the whole point of this
/// crate. `2^53 + 1` written into a document survives being read back; through
/// a double it does not, and the corruption is silent. A balance in minor units
/// is exactly the value a sender writes as a large integer.
///
/// The literal's **shape** decides the variant, not its value: `1.0` is a
/// [`Float`] even though it is integral, because a document that says `1.0`
/// said "measurement", not "count".
///
/// A `Float` is always finite. `NaN` and the infinities are unrepresentable in
/// JSON, so they never enter the tree: the parser refuses a literal that
/// overflows, and [`Number::from_f64`] returns `None`.
///
/// [`Float`]: Number::Float
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    /// A non-negative integer literal.
    PosInt(u64),
    /// A negative integer literal.
    NegInt(i64),
    /// A literal carrying a fraction or an exponent. Always finite.
    Float(f64),
}

impl Number {
    /// Build a float, refusing anything not finite.
    #[must_use]
    pub fn from_f64(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self::Float(value))
    }

    /// The value as `u64`, only when it is exactly that.
    ///
    /// A negative integer and a float both give `None`. A caller asking for an
    /// exact unsigned integer never silently receives a rounded float.
    #[must_use]
    pub const fn as_u64(self) -> Option<u64> {
        match self {
            Self::PosInt(v) => Some(v),
            _ => None,
        }
    }

    /// The value as `i64`, only when it is exactly that.
    #[must_use]
    #[allow(
        clippy::cast_possible_wrap,
        reason = "the guard is `v <= i64::MAX`, so the cast cannot wrap"
    )]
    pub const fn as_i64(self) -> Option<i64> {
        match self {
            Self::PosInt(v) if v <= i64::MAX as u64 => Some(v as i64),
            Self::NegInt(v) => Some(v),
            _ => None,
        }
    }

    /// The value as `f64`.
    ///
    /// Always succeeds, including for integers — asking for a float is an
    /// explicit request to accept float behaviour, and the loss is the
    /// caller's to declare. The reverse conversion is the one that must not be
    /// implicit, and is not.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "asking for a float IS the request to accept it; as_u64/as_i64 refuse"
    )]
    pub fn as_f64(self) -> f64 {
        match self {
            Self::PosInt(v) => v as f64,
            Self::NegInt(v) => v as f64,
            Self::Float(v) => v,
        }
    }

    /// Whether this number came from an integer literal.
    #[must_use]
    pub const fn is_integer(self) -> bool {
        matches!(self, Self::PosInt(_) | Self::NegInt(_))
    }
}

/// A JSON value.
///
/// # Dropping is iterative
///
/// [`Value`] implements [`Drop`] so that a deeply nested tree is dismantled
/// with an explicit worklist instead of by recursion. Without it, dropping a
/// value a few hundred thousand levels deep overflows the stack — and a stack
/// overflow is an abort, not something a caller can catch. Parsing caps depth,
/// but a value built in code is not capped.
///
/// The cost is that `Value` cannot be destructured by move. Use [`as_array`],
/// [`as_object`], [`into_array`], [`into_object`] and friends instead.
///
/// [`as_array`]: Value::as_array
/// [`as_object`]: Value::as_object
/// [`into_array`]: Value::into_array
/// [`into_object`]: Value::into_object
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    /// `null`.
    #[default]
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(Number),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object.
    Object(Map),
}

impl Drop for Value {
    fn drop(&mut self) {
        // Move every child out into a flat worklist, then let each drop with
        // its own children already removed. Depth of recursion: two.
        let mut stack: Vec<Value> = Vec::new();
        take_children(self, &mut stack);
        while let Some(mut value) = stack.pop() {
            take_children(&mut value, &mut stack);
        }
    }
}

fn take_children(value: &mut Value, stack: &mut Vec<Value>) {
    match value {
        Value::Array(items) => stack.append(items),
        Value::Object(map) => map.drain_values_into(stack),
        _ => {}
    }
}

impl Value {
    /// `true` when this is `null`.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// The boolean, when this is one.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// The string, when this is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(v) => Some(v),
            _ => None,
        }
    }

    /// The number, when this is one.
    #[must_use]
    pub const fn as_number(&self) -> Option<Number> {
        match self {
            Self::Number(v) => Some(*v),
            _ => None,
        }
    }

    /// An exact `u64`, when this is an integer that fits.
    #[must_use]
    pub const fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(n) => n.as_u64(),
            _ => None,
        }
    }

    /// An exact `i64`, when this is an integer that fits.
    #[must_use]
    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(n) => n.as_i64(),
            _ => None,
        }
    }

    /// An `f64`, when this is any number.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(n.as_f64()),
            _ => None,
        }
    }

    /// The array, when this is one.
    #[must_use]
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }

    /// The array, mutably.
    #[must_use]
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Self::Array(v) => Some(v),
            _ => None,
        }
    }

    /// The object, when this is one.
    #[must_use]
    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Self::Object(v) => Some(v),
            _ => None,
        }
    }

    /// The object, mutably.
    #[must_use]
    pub fn as_object_mut(&mut self) -> Option<&mut Map> {
        match self {
            Self::Object(v) => Some(v),
            _ => None,
        }
    }

    /// Take the array out, leaving `null` behind.
    ///
    /// The by-value accessor, because [`Value`] implements [`Drop`] and so
    /// cannot be destructured by move.
    #[must_use]
    pub fn into_array(mut self) -> Option<Vec<Value>> {
        match &mut self {
            Self::Array(v) => Some(core::mem::take(v)),
            _ => None,
        }
    }

    /// Take the object out, leaving `null` behind.
    #[must_use]
    pub fn into_object(mut self) -> Option<Map> {
        match &mut self {
            Self::Object(v) => Some(core::mem::take(v)),
            _ => None,
        }
    }

    /// Take the string out, leaving `null` behind.
    #[must_use]
    pub fn into_string(mut self) -> Option<String> {
        match &mut self {
            Self::String(v) => Some(core::mem::take(v)),
            _ => None,
        }
    }

    /// Replace this value with `null`, returning what was here.
    #[must_use]
    pub fn take(&mut self) -> Value {
        core::mem::take(self)
    }

    /// One key of an object. `None` for any other kind of value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }

    /// One element of an array. `None` for any other kind of value.
    #[must_use]
    pub fn at(&self, index: usize) -> Option<&Value> {
        self.as_array()?.get(index)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(String::from(value))
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<Number> for Value {
    fn from(value: Number) -> Self {
        Self::Number(value)
    }
}

macro_rules! from_unsigned {
    ($($t:ty),*) => {$(
        impl From<$t> for Value {
            fn from(value: $t) -> Self {
                Self::Number(Number::PosInt(u64::from(value)))
            }
        }
    )*};
}
from_unsigned!(u8, u16, u32, u64);

macro_rules! from_signed {
    ($($t:ty),*) => {$(
        impl From<$t> for Value {
            fn from(value: $t) -> Self {
                let value = i64::from(value);
                // A non-negative signed integer becomes `PosInt`, so
                // `Value::from(0)` and a parsed `0` are the same value. Two
                // representations of one number would make equality lie.
                Self::Number(if value < 0 {
                    Number::NegInt(value)
                } else {
                    #[allow(clippy::cast_sign_loss)]
                    Number::PosInt(value as u64)
                })
            }
        }
    )*};
}
from_signed!(i8, i16, i32, i64);

impl From<f64> for Value {
    /// A non-finite float becomes [`Value::Null`].
    ///
    /// `NaN` and the infinities have no JSON spelling. `JSON.stringify` emits
    /// `null` for them and this matches, rather than panicking inside a
    /// `From` impl that cannot report failure. Use [`Number::from_f64`] when
    /// the caller needs to know.
    fn from(value: f64) -> Self {
        Number::from_f64(value).map_or(Self::Null, Self::Number)
    }
}

/// A JSON object: string keys to values, **in insertion order**.
///
/// Order is preserved because every peer runtime preserves it — a PHP array, a
/// Python dict and a JavaScript object all round-trip a document with its keys
/// where the author put them. Sorting belongs in the writer, where
/// [`to_string_canonical`] puts it, not in the container.
///
/// Lookup is `O(log n)` through a side index rather than a linear scan. A scan
/// is fine for the small objects JSON usually carries and quadratic on a
/// hostile document with a hundred thousand keys — which is exactly the input
/// this crate is meant to survive.
///
/// [`to_string_canonical`]: crate::to_string_canonical
#[derive(Debug, Clone, Default)]
pub struct Map {
    entries: Vec<(String, Value)>,
    index: BTreeMap<String, usize>,
}

impl Map {
    /// An empty object.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty object with room for `capacity` entries.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            index: BTreeMap::new(),
        }
    }

    /// How many keys the object has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the object has no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert or replace a key.
    ///
    /// A repeated key keeps its **first position** and takes the **last
    /// value** — what JavaScript, PHP and Python all do. Returns the value
    /// that was displaced.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        let key = key.into();
        if let Some(&at) = self.index.get(&key) {
            return Some(core::mem::replace(&mut self.entries[at].1, value));
        }
        self.index.insert(key.clone(), self.entries.len());
        self.entries.push((key, value));
        None
    }

    /// Look up a key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.index.get(key).map(|&at| &self.entries[at].1)
    }

    /// Look up a key, mutably.
    #[must_use]
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        let at = *self.index.get(key)?;
        Some(&mut self.entries[at].1)
    }

    /// Whether a key is present.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// Remove a key, returning its value.
    ///
    /// `O(n)`: removal shifts every later entry, so the side index is rebuilt.
    /// Removal is rare in this crate's use, and a stale index would be a
    /// silently wrong lookup.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let at = self.index.remove(key)?;
        let (_, value) = self.entries.remove(at);
        for slot in self.index.values_mut() {
            if *slot > at {
                *slot -= 1;
            }
        }
        Some(value)
    }

    /// The keys, in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(key, _)| key.as_str())
    }

    /// The values, in insertion order.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().map(|(_, value)| value)
    }

    /// The entries, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    /// The entries, sorted by key (by Unicode scalar value).
    ///
    /// What [`to_string_canonical`] walks.
    ///
    /// [`to_string_canonical`]: crate::to_string_canonical
    #[must_use]
    pub fn sorted_entries(&self) -> Vec<(&str, &Value)> {
        let mut out: Vec<(&str, &Value)> =
            self.entries.iter().map(|(k, v)| (k.as_str(), v)).collect();
        out.sort_unstable_by(|a, b| a.0.cmp(b.0));
        out
    }

    /// Move every value out into `sink`, leaving the object empty.
    ///
    /// Used by [`Value`]'s iterative `Drop`.
    pub(crate) fn drain_values_into(&mut self, sink: &mut Vec<Value>) {
        sink.extend(self.entries.drain(..).map(|(_, value)| value));
        self.index.clear();
    }
}

impl PartialEq for Map {
    /// Key order does **not** affect equality.
    ///
    /// Two documents that list the same pairs in different orders are the same
    /// JSON value; order is authored presentation, which is why it survives a
    /// round trip but does not decide equality. The conformance loaders in
    /// every other language compare objects the same way.
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .all(|(key, value)| other.get(key) == Some(value))
    }
}

impl<K: Into<String>> FromIterator<(K, Value)> for Map {
    fn from_iter<I: IntoIterator<Item = (K, Value)>>(iter: I) -> Self {
        let mut map = Self::new();
        for (key, value) in iter {
            map.insert(key, value);
        }
        map
    }
}
