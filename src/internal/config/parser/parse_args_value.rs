use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ParseArgsValue {
    SingleString(Option<String>),
    SingleInteger(Option<i64>),
    SingleFloat(Option<f64>),
    SingleBoolean(Option<bool>),
    ManyString(Vec<Option<String>>),
    ManyInteger(Vec<Option<i64>>),
    ManyFloat(Vec<Option<f64>>),
    ManyBoolean(Vec<Option<bool>>),
    GroupedString(Vec<Vec<Option<String>>>),
    GroupedInteger(Vec<Vec<Option<i64>>>),
    GroupedFloat(Vec<Vec<Option<f64>>>),
    GroupedBoolean(Vec<Vec<Option<bool>>>),
}

impl ParseArgsValue {
    pub fn export_to_env(&self, key: &str, args: &mut BTreeMap<String, String>) {
        let type_key = format!("OMNI_ARG_{}_TYPE", key.to_uppercase());
        let value_key = format!("OMNI_ARG_{}_VALUE", key.to_uppercase());

        match self {
            Self::SingleString(value) => {
                args.insert(type_key, "str".to_string());
                if let Some(value) = value {
                    if !value.is_empty() {
                        args.insert(value_key, value.clone());
                    }
                }
            }
            Self::SingleInteger(value) => {
                args.insert(type_key, "int".to_string());
                if let Some(value) = value {
                    args.insert(value_key, value.to_string());
                }
            }
            Self::SingleFloat(value) => {
                args.insert(type_key, "float".to_string());
                if let Some(value) = value {
                    args.insert(value_key, value.to_string());
                }
            }
            Self::SingleBoolean(value) => {
                args.insert(type_key, "bool".to_string());
                if let Some(value) = value {
                    args.insert(value_key, value.to_string());
                }
            }
            Self::ManyString(values) => {
                args.insert(type_key, format!("str/{}", values.len()));
                for (idx, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        if !value.is_empty() {
                            args.insert(format!("{}_{}", value_key, idx), value.clone());
                        }
                    }
                }
            }
            Self::ManyInteger(values) => {
                args.insert(type_key, format!("int/{}", values.len()));
                for (idx, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        args.insert(format!("{}_{}", value_key, idx), value.to_string());
                    }
                }
            }
            Self::ManyFloat(values) => {
                args.insert(type_key, format!("float/{}", values.len()));
                for (idx, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        args.insert(format!("{}_{}", value_key, idx), value.to_string());
                    }
                }
            }
            Self::ManyBoolean(values) => {
                args.insert(type_key, format!("bool/{}", values.len()));
                for (idx, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        args.insert(format!("{}_{}", value_key, idx), value.to_string());
                    }
                }
            }
            Self::GroupedString(values) => {
                let mut max_occurrences = 0;
                for (idx, values) in values.iter().enumerate() {
                    max_occurrences = max_occurrences.max(values.len());
                    args.insert(
                        format!("{}_{}", type_key, idx),
                        format!("str/{}", values.len()),
                    );

                    for (jdx, value) in values.iter().enumerate() {
                        if let Some(value) = value {
                            if !value.is_empty() {
                                args.insert(
                                    format!("{}_{}_{}", value_key, idx, jdx),
                                    value.clone(),
                                );
                            }
                        }
                    }
                }
                args.insert(
                    type_key,
                    format!("str/{}/{}", values.len(), max_occurrences),
                );
            }
            Self::GroupedInteger(values) => {
                let mut max_occurrences = 0;
                for (idx, values) in values.iter().enumerate() {
                    max_occurrences = max_occurrences.max(values.len());
                    args.insert(
                        format!("{}_{}", type_key, idx),
                        format!("int/{}", values.len()),
                    );

                    for (jdx, value) in values.iter().enumerate() {
                        if let Some(value) = value {
                            args.insert(
                                format!("{}_{}_{}", value_key, idx, jdx),
                                value.to_string(),
                            );
                        }
                    }
                }
                args.insert(
                    type_key,
                    format!("int/{}/{}", values.len(), max_occurrences),
                );
            }
            Self::GroupedFloat(values) => {
                let mut max_occurrences = 0;
                for (idx, values) in values.iter().enumerate() {
                    max_occurrences = max_occurrences.max(values.len());
                    args.insert(
                        format!("{}_{}", type_key, idx),
                        format!("float/{}", values.len()),
                    );

                    for (jdx, value) in values.iter().enumerate() {
                        if let Some(value) = value {
                            args.insert(
                                format!("{}_{}_{}", value_key, idx, jdx),
                                value.to_string(),
                            );
                        }
                    }
                }
                args.insert(
                    type_key,
                    format!("float/{}/{}", values.len(), max_occurrences),
                );
            }
            Self::GroupedBoolean(values) => {
                let mut max_occurrences = 0;
                for (idx, values) in values.iter().enumerate() {
                    max_occurrences = max_occurrences.max(values.len());
                    args.insert(
                        format!("{}_{}", type_key, idx),
                        format!("bool/{}", values.len()),
                    );

                    for (jdx, value) in values.iter().enumerate() {
                        if let Some(value) = value {
                            args.insert(
                                format!("{}_{}_{}", value_key, idx, jdx),
                                value.to_string(),
                            );
                        }
                    }
                }
                args.insert(
                    type_key,
                    format!("bool/{}/{}", values.len(), max_occurrences),
                );
            }
        }
    }
}

impl From<&str> for ParseArgsValue {
    fn from(value: &str) -> Self {
        Self::SingleString(Some(value.to_string()))
    }
}

impl From<Option<&str>> for ParseArgsValue {
    fn from(value: Option<&str>) -> Self {
        match value {
            Some(value) => Self::SingleString(Some(value.to_string())),
            None => Self::SingleString(None),
        }
    }
}

impl From<String> for ParseArgsValue {
    fn from(value: String) -> Self {
        Self::SingleString(Some(value))
    }
}

impl From<Option<String>> for ParseArgsValue {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(value) => Self::SingleString(Some(value)),
            None => Self::SingleString(None),
        }
    }
}

impl From<i64> for ParseArgsValue {
    fn from(value: i64) -> Self {
        Self::SingleInteger(Some(value))
    }
}

impl From<Option<i64>> for ParseArgsValue {
    fn from(value: Option<i64>) -> Self {
        match value {
            Some(value) => Self::SingleInteger(Some(value)),
            None => Self::SingleInteger(None),
        }
    }
}

impl From<u8> for ParseArgsValue {
    fn from(value: u8) -> Self {
        Self::SingleInteger(Some(value as i64))
    }
}

impl From<Option<u8>> for ParseArgsValue {
    fn from(value: Option<u8>) -> Self {
        match value {
            Some(value) => Self::SingleInteger(Some(value as i64)),
            None => Self::SingleInteger(None),
        }
    }
}

impl From<f64> for ParseArgsValue {
    fn from(value: f64) -> Self {
        Self::SingleFloat(Some(value))
    }
}

impl From<Option<f64>> for ParseArgsValue {
    fn from(value: Option<f64>) -> Self {
        match value {
            Some(value) => Self::SingleFloat(Some(value)),
            None => Self::SingleFloat(None),
        }
    }
}

impl From<bool> for ParseArgsValue {
    fn from(value: bool) -> Self {
        Self::SingleBoolean(Some(value))
    }
}

impl From<Option<bool>> for ParseArgsValue {
    fn from(value: Option<bool>) -> Self {
        match value {
            Some(value) => Self::SingleBoolean(Some(value)),
            None => Self::SingleBoolean(None),
        }
    }
}

impl From<Vec<Option<&str>>> for ParseArgsValue {
    fn from(value: Vec<Option<&str>>) -> Self {
        Self::ManyString(
            value
                .iter()
                .map(|value| value.map(|value| value.to_string()))
                .collect(),
        )
    }
}

impl From<Vec<String>> for ParseArgsValue {
    fn from(value: Vec<String>) -> Self {
        Self::ManyString(value.into_iter().map(|value| Some(value)).collect())
    }
}

impl From<Vec<Option<String>>> for ParseArgsValue {
    fn from(value: Vec<Option<String>>) -> Self {
        Self::ManyString(value)
    }
}

impl From<Vec<i64>> for ParseArgsValue {
    fn from(value: Vec<i64>) -> Self {
        Self::ManyInteger(value.into_iter().map(|value| Some(value)).collect())
    }
}

impl From<Vec<Option<i64>>> for ParseArgsValue {
    fn from(value: Vec<Option<i64>>) -> Self {
        Self::ManyInteger(value)
    }
}

impl From<Vec<u8>> for ParseArgsValue {
    fn from(value: Vec<u8>) -> Self {
        Self::ManyInteger(value.into_iter().map(|value| Some(value as i64)).collect())
    }
}

impl From<Vec<Option<u8>>> for ParseArgsValue {
    fn from(value: Vec<Option<u8>>) -> Self {
        Self::ManyInteger(
            value
                .into_iter()
                .map(|value| value.map(|value| value as i64))
                .collect(),
        )
    }
}

impl From<Vec<f64>> for ParseArgsValue {
    fn from(value: Vec<f64>) -> Self {
        Self::ManyFloat(value.into_iter().map(|value| Some(value)).collect())
    }
}

impl From<Vec<Option<f64>>> for ParseArgsValue {
    fn from(value: Vec<Option<f64>>) -> Self {
        Self::ManyFloat(value)
    }
}

impl From<Vec<bool>> for ParseArgsValue {
    fn from(value: Vec<bool>) -> Self {
        Self::ManyBoolean(value.into_iter().map(|value| Some(value)).collect())
    }
}

impl From<Vec<Option<bool>>> for ParseArgsValue {
    fn from(value: Vec<Option<bool>>) -> Self {
        Self::ManyBoolean(value)
    }
}

impl From<Vec<Vec<&str>>> for ParseArgsValue {
    fn from(value: Vec<Vec<&str>>) -> Self {
        Self::GroupedString(
            value
                .into_iter()
                .map(|values| {
                    values
                        .into_iter()
                        .map(|value| Some(value.to_string()))
                        .collect()
                })
                .collect(),
        )
    }
}

impl From<Vec<Vec<Option<&str>>>> for ParseArgsValue {
    fn from(value: Vec<Vec<Option<&str>>>) -> Self {
        Self::GroupedString(
            value
                .into_iter()
                .map(|values| {
                    values
                        .into_iter()
                        .map(|value| value.map(|value| value.to_string()))
                        .collect()
                })
                .collect(),
        )
    }
}

impl From<Vec<Vec<String>>> for ParseArgsValue {
    fn from(value: Vec<Vec<String>>) -> Self {
        Self::GroupedString(
            value
                .into_iter()
                .map(|values| values.into_iter().map(|value| Some(value)).collect())
                .collect(),
        )
    }
}

impl From<Vec<Vec<Option<String>>>> for ParseArgsValue {
    fn from(value: Vec<Vec<Option<String>>>) -> Self {
        Self::GroupedString(value)
    }
}

impl From<Vec<Vec<i64>>> for ParseArgsValue {
    fn from(value: Vec<Vec<i64>>) -> Self {
        Self::GroupedInteger(
            value
                .into_iter()
                .map(|values| values.into_iter().map(|value| Some(value)).collect())
                .collect(),
        )
    }
}

impl From<Vec<Vec<Option<i64>>>> for ParseArgsValue {
    fn from(value: Vec<Vec<Option<i64>>>) -> Self {
        Self::GroupedInteger(value)
    }
}

impl From<Vec<Vec<u8>>> for ParseArgsValue {
    fn from(value: Vec<Vec<u8>>) -> Self {
        Self::GroupedInteger(
            value
                .into_iter()
                .map(|values| values.into_iter().map(|value| Some(value as i64)).collect())
                .collect(),
        )
    }
}

impl From<Vec<Vec<Option<u8>>>> for ParseArgsValue {
    fn from(value: Vec<Vec<Option<u8>>>) -> Self {
        Self::GroupedInteger(
            value
                .into_iter()
                .map(|values| {
                    values
                        .into_iter()
                        .map(|value| value.map(|value| value as i64))
                        .collect()
                })
                .collect(),
        )
    }
}

impl From<Vec<Vec<f64>>> for ParseArgsValue {
    fn from(value: Vec<Vec<f64>>) -> Self {
        Self::GroupedFloat(
            value
                .into_iter()
                .map(|values| values.into_iter().map(|value| Some(value)).collect())
                .collect(),
        )
    }
}

impl From<Vec<Vec<Option<f64>>>> for ParseArgsValue {
    fn from(value: Vec<Vec<Option<f64>>>) -> Self {
        Self::GroupedFloat(value)
    }
}

impl From<Vec<Vec<bool>>> for ParseArgsValue {
    fn from(value: Vec<Vec<bool>>) -> Self {
        Self::GroupedBoolean(
            value
                .into_iter()
                .map(|values| values.into_iter().map(|value| Some(value)).collect())
                .collect(),
        )
    }
}

impl From<Vec<Vec<Option<bool>>>> for ParseArgsValue {
    fn from(value: Vec<Vec<Option<bool>>>) -> Self {
        Self::GroupedBoolean(value)
    }
}
