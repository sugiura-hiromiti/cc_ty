use serde_json::Map as JsonMap;
use serde_json::Number as JsonNumber;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::io::{self};
use std::path::Path;

#[derive(Debug,)]
pub enum ConvertError {
	Io(io::Error,),
	Yaml(serde_yaml::Error,),
	Json(serde_json::Error,),
	InvalidKeyType { path: String, key: String, },
	InvalidColorValue { path: String, detail: String, },
}

impl fmt::Display for ConvertError {
	fn fmt(&self, f: &mut fmt::Formatter<'_,>,) -> fmt::Result {
		match self {
			ConvertError::Io(err,) => write!(f, "I/O error: {}", err),
			ConvertError::Yaml(err,) => write!(f, "YAML parse error: {}", err),
			ConvertError::Json(err,) => write!(f, "JSON write error: {}", err),
			ConvertError::InvalidKeyType { path, key, } => {
				write!(f, "unsupported mapping key {} at {}", key, path)
			},
			ConvertError::InvalidColorValue { path, detail, } => {
				write!(f, "invalid color value at {}: {}", path, detail)
			},
		}
	}
}

impl std::error::Error for ConvertError {
	fn source(&self,) -> Option<&(dyn std::error::Error + 'static),> {
		match self {
			ConvertError::Io(err,) => Some(err,),
			ConvertError::Yaml(err,) => Some(err,),
			ConvertError::Json(err,) => Some(err,),
			ConvertError::InvalidKeyType { .. } => None,
			ConvertError::InvalidColorValue { .. } => None,
		}
	}
}

impl From<io::Error,> for ConvertError {
	fn from(value: io::Error,) -> Self {
		ConvertError::Io(value,)
	}
}

impl From<serde_yaml::Error,> for ConvertError {
	fn from(value: serde_yaml::Error,) -> Self {
		ConvertError::Yaml(value,)
	}
}

impl From<serde_json::Error,> for ConvertError {
	fn from(value: serde_json::Error,) -> Self {
		ConvertError::Json(value,)
	}
}

fn convert_yaml_value(
	value: &YamlValue,
	path: &mut Vec<String,>,
) -> Result<JsonValue, ConvertError,> {
	match value {
		YamlValue::Mapping(mapping,) => {
			let mut out = JsonMap::with_capacity(mapping.len(),);
			for (key, child,) in mapping {
				let key_string = yaml_key_to_string(key,).ok_or_else(|| {
					ConvertError::InvalidKeyType {
						path: format_path(path,),
						key:  format!("{:?}", key),
					}
				},)?;

				path.push(key_string.clone(),);
				let converted = convert_yaml_value(child, path,)?;
				path.pop();

				out.insert(key_string, converted,);
			}
			Ok(JsonValue::Object(out,),)
		},
		YamlValue::Sequence(items,) => {
			let rgb = sequence_to_rgb(items, path,)?;
			Ok(JsonValue::String(format!(
				"#{:02X}{:02X}{:02X}",
				rgb[0], rgb[1], rgb[2]
			),),)
		},
		YamlValue::Null => Ok(JsonValue::Null,),
		YamlValue::Bool(flag,) => Ok(JsonValue::Bool(*flag,),),
		YamlValue::Number(number,) => {
			convert_yaml_number(number,).ok_or_else(|| {
				ConvertError::InvalidColorValue {
					path:   format_path(path,),
					detail: "encountered non-finite numeric value".to_string(),
				}
			},)
		},
		YamlValue::String(text,) => Ok(JsonValue::String(text.clone(),),),
		YamlValue::Tagged(tagged,) => convert_yaml_value(&tagged.value, path,),
	}
}

fn convert_yaml_number(number: &serde_yaml::Number,) -> Option<JsonValue,> {
	if let Some(u,) = number.as_u64() {
		return Some(JsonValue::Number(JsonNumber::from(u,),),);
	}
	if let Some(i,) = number.as_i64() {
		return Some(JsonValue::Number(JsonNumber::from(i,),),);
	}
	number.as_f64().and_then(JsonNumber::from_f64,).map(JsonValue::Number,)
}

fn sequence_to_rgb(
	values: &[YamlValue],
	path: &mut Vec<String,>,
) -> Result<[u8; 3], ConvertError,> {
	if values.len() != 3 {
		return Err(ConvertError::InvalidColorValue {
			path:   format_path(path,),
			detail: format!(
				"expected exactly 3 RGB components, found {}",
				values.len()
			),
		},);
	}

	let mut rgb = [0u8; 3];
	for (idx, value,) in values.iter().enumerate() {
		let component = match value {
			YamlValue::Number(number,) => {
				if let Some(u,) = number.as_u64() {
					if u > 255 {
						return Err(ConvertError::InvalidColorValue {
							path:   format_path(path,),
							detail: format!(
								"channel {} must be between 0 and 255, found \
								 {}",
								idx, u
							),
						},);
					}
					u as u8
				} else if let Some(i,) = number.as_i64() {
					if !(0..=255).contains(&i,) {
						return Err(ConvertError::InvalidColorValue {
							path:   format_path(path,),
							detail: format!(
								"channel {} must be between 0 and 255, found \
								 {}",
								idx, i
							),
						},);
					}
					i as u8
				} else {
					return Err(ConvertError::InvalidColorValue {
						path:   format_path(path,),
						detail: format!("channel {} is not an integer", idx),
					},);
				}
			},
			_ => {
				return Err(ConvertError::InvalidColorValue {
					path:   format_path(path,),
					detail: format!("channel {} is not a numeric value", idx),
				},);
			},
		};

		rgb[idx] = component;
	}

	Ok(rgb,)
}

fn yaml_key_to_string(value: &YamlValue,) -> Option<String,> {
	match value {
		YamlValue::String(s,) => Some(s.clone(),),
		YamlValue::Number(n,) => Some(n.to_string(),),
		YamlValue::Bool(b,) => Some(b.to_string(),),
		_ => None,
	}
}

fn format_path(path: &[String],) -> String {
	if path.is_empty() { "<root>".to_string() } else { path.join(".",) }
}

/// Convert a reader containing color YAML data into a JSON value with hex
/// colors.
pub fn convert_reader<R: Read,>(
	reader: R,
) -> Result<JsonValue, ConvertError,> {
	let yaml: YamlValue = serde_yaml::from_reader(reader,)?;
	let mut path = Vec::new();
	convert_yaml_value(&yaml, &mut path,)
}

/// Convenience helper to convert raw YAML text into a JSON value.
pub fn convert_str(yaml: &str,) -> Result<JsonValue, ConvertError,> {
	let yaml: YamlValue = serde_yaml::from_str(yaml,)?;
	let mut path = Vec::new();
	convert_yaml_value(&yaml, &mut path,)
}

/// Convert YAML color information into JSON, writing the result to the writer.
pub fn convert_to_writer<R: Read, W: Write,>(
	reader: R,
	mut writer: W,
) -> Result<(), ConvertError,> {
	let json = convert_reader(reader,)?;
	serde_json::to_writer_pretty(&mut writer, &json,)?;
	writer.write_all(b"\n",)?;
	Ok((),)
}

/// Convert the colors stored at `input` and persist the JSON representation to
/// `output`.
pub fn convert_file(
	input: impl AsRef<Path,>,
	output: impl AsRef<Path,>,
) -> Result<(), ConvertError,> {
	let reader = File::open(input,)?;
	let json = convert_reader(reader,)?;
	let mut writer = File::create(output,)?;
	serde_json::to_writer_pretty(&mut writer, &json,)?;
	writer.write_all(b"\n",)?;
	Ok((),)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn converts_sample_palette() {
		let yaml = r#"
        dark:
          normal:
            red:
              - 171
              - 76
              - 63
        "#;

		let json = convert_str(yaml,).expect("conversion should succeed",);
		let red = json
			.get("dark",)
			.and_then(|v| v.get("normal",),)
			.and_then(|v| v.get("red",),)
			.and_then(JsonValue::as_str,)
			.unwrap();

		assert_eq!(red, "#AB4C3F");
	}

	#[test]
	fn rejects_incomplete_color() {
		let yaml = r#"
        palette:
          accent:
            - 255
            - 0
        "#;

		let error = convert_str(yaml,).expect_err("expected validation error",);
		match error {
			ConvertError::InvalidColorValue { path, .. } => {
				assert_eq!(path, "palette.accent")
			},
			other => panic!("unexpected error: {:?}", other),
		}
	}
}
