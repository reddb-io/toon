// Typed serde bridge: any `Serialize` value encodes to TOON, and TOON decodes
// into any `DeserializeOwned` type. Both directions pass through the JSON data
// model (§2), so serde attributes (`rename`, `skip_serializing_if`, …) shape
// the document exactly as they shape JSON.

/// Why a typed [`to_string`] or [`from_str`] conversion failed.
#[cfg(feature = "serde")]
#[derive(Debug)]
#[non_exhaustive]
pub enum SerdeError {
    /// The TOON text was malformed.
    Decode(DecodeError),
    /// The value could not be encoded as TOON.
    Encode(EncodeError),
    /// The value does not map onto the JSON data model, or the decoded
    /// document does not fit the target type.
    Data(serde_json::Error),
}

#[cfg(feature = "serde")]
impl fmt::Display for SerdeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
            Self::Data(error) => error.fmt(formatter),
        }
    }
}

#[cfg(feature = "serde")]
impl std::error::Error for SerdeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::Data(error) => Some(error),
        }
    }
}

#[cfg(feature = "serde")]
impl From<DecodeError> for SerdeError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

#[cfg(feature = "serde")]
impl From<EncodeError> for SerdeError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

#[cfg(feature = "serde")]
impl From<serde_json::Error> for SerdeError {
    fn from(error: serde_json::Error) -> Self {
        Self::Data(error)
    }
}

/// Encodes any `Serialize` value as a canonical TOON document.
///
/// ```
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize, Debug, PartialEq)]
/// struct User {
///     id: u32,
///     name: String,
/// }
///
/// let users = vec![User { id: 1, name: "Ada".into() }, User { id: 2, name: "Linus".into() }];
/// let toon = reddb_io_toon::to_string(&users)?;
/// assert_eq!(toon, "[2]{id,name}:\n  1,Ada\n  2,Linus");
///
/// let back: Vec<User> = reddb_io_toon::from_str(&toon)?;
/// assert_eq!(back, users);
/// # Ok::<(), reddb_io_toon::SerdeError>(())
/// ```
#[cfg(feature = "serde")]
pub fn to_string<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, SerdeError> {
    to_string_with_options(value, EncodeOptions::default())
}

/// Encodes any `Serialize` value as TOON with explicit encoder options.
#[cfg(feature = "serde")]
pub fn to_string_with_options<T: serde::Serialize + ?Sized>(
    value: &T,
    options: EncodeOptions,
) -> Result<String, SerdeError> {
    let json = serde_json::to_value(value)?;
    Ok(encode_with_options(&Value::from_json_value(json), options)?)
}

/// Decodes a TOON document into any `DeserializeOwned` type.
#[cfg(feature = "serde")]
pub fn from_str<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, SerdeError> {
    from_str_with_options(input, &DecodeOptions::default())
}

/// Decodes a TOON document into any `DeserializeOwned` type with explicit
/// decoder options, such as the input limits for untrusted text.
#[cfg(feature = "serde")]
pub fn from_str_with_options<T: serde::de::DeserializeOwned>(
    input: &str,
    options: &DecodeOptions,
) -> Result<T, SerdeError> {
    let value = decode_with_options(input, options)?;
    Ok(serde_json::from_value(value.to_json_value())?)
}
