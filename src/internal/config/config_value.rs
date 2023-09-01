use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;
use serde_yaml;

use crate::internal::env::ENV;
use crate::internal::env::HOME;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigSource {
    Default,
    File(String),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigData {
    Mapping(HashMap<String, ConfigValue>),
    Sequence(Vec<ConfigValue>),
    Value(serde_yaml::Value),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigExtendStrategy {
    Default,
    Append,
    Prepend,
    Replace,
    Keep,
    Raw,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigValue {
    source: ConfigSource,
    labels: Vec<String>,
    value: Option<Box<ConfigData>>,
}

impl AsRef<ConfigData> for ConfigValue {
    fn as_ref(&self) -> &ConfigData {
        self.value
            .as_ref()
            .expect("ConfigValue does not contain a value")
    }
}

impl std::fmt::Display for ConfigValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self.unwrap()).unwrap())
    }
}

impl ConfigValue {
    fn new(source: ConfigSource, labels: Vec<String>, value: Option<Box<ConfigData>>) -> Self {
        Self {
            source,
            labels,
            value,
        }
    }

    pub fn new_null(source: ConfigSource, labels: Vec<String>) -> Self {
        Self::new(
            source,
            labels,
            Some(Box::new(ConfigData::Value(serde_yaml::Value::Null))),
        )
    }

    pub fn default() -> Self {
        // Check if ~/git exists and is a directory
        let default_cache_path = format!("{}", ENV.cache_home);
        let default_cache_config = format!("cache:\n  path: \"{}\"\n", default_cache_path);

        // Parse a default yaml file using serde
        let yaml_str = default_cache_config
            + r#"
worktree: null
commands: {}
command_match_min_score: 0.12
command_match_skip_prompt_if:
  enabled: true
  first_min: 0.80
  second_max: 0.60
cd:
  path_match_min_score: 0.12
  path_match_skip_prompt_if:
    enabled: true
    first_min: 0.80
    second_max: 0.60
clone:
  ls_remote_timeout_seconds: 5
config_commands:
  split_on_dash: true
  split_on_slash: true
env: {}
makefile_commands:
  enabled: true
  split_on_dash: true
  split_on_slash: true
org: []
path:
  append: []
  prepend: []
path_repo_updates:
  enabled: true
  self_update: ask
  interval: 43200 # 12 hours
  ref_type: "branch" # branch or tag
  ref_match: null # regex or null
  per_repo_config: {}
repo_path_format: "%{host}/%{org}/%{repo}"
"#;

        // Convert yaml_str from String to &str
        let yaml_str = yaml_str.as_str();

        let value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        Self::from_value(ConfigSource::Default, vec!["default".to_string()], value)
    }

    pub fn from_value(source: ConfigSource, labels: Vec<String>, value: serde_yaml::Value) -> Self {
        let config_data = match value {
            serde_yaml::Value::Mapping(mapping) => {
                ConfigData::Mapping(Self::from_mapping(source.clone(), labels.clone(), mapping))
            }
            serde_yaml::Value::Sequence(sequence) => ConfigData::Sequence(Self::from_sequence(
                source.clone(),
                labels.clone(),
                sequence,
            )),
            _ => ConfigData::Value(value),
        };
        Self::new(source, labels, Some(Box::new(config_data)))
    }

    fn from_mapping(
        source: ConfigSource,
        labels: Vec<String>,
        mapping: serde_yaml::Mapping,
    ) -> HashMap<String, ConfigValue> {
        let mut config_mapping = HashMap::new();
        for (key, value) in mapping {
            let new_value = ConfigValue::from_value(source.clone(), labels.clone(), value);
            config_mapping.insert(key.as_str().unwrap().to_string(), new_value);
        }
        config_mapping
    }

    fn from_sequence(
        source: ConfigSource,
        labels: Vec<String>,
        sequence: serde_yaml::Sequence,
    ) -> Vec<ConfigValue> {
        let mut config_mapping = Vec::new();
        for value in sequence {
            let new_value = ConfigValue::from_value(source.clone(), labels.clone(), value);
            config_mapping.push(new_value);
        }
        config_mapping
    }

    pub fn from_str(value: &str) -> Self {
        let value: serde_yaml::Value = serde_yaml::from_str(value).unwrap();
        Self::from_value(ConfigSource::Null, vec![], value)
    }

    pub fn add_label(&mut self, label: &str) {
        self.labels.push(label.to_owned());
        if let Some(data) = self.value.as_mut().map(|data| data.as_mut()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    for (_, ref mut value) in mapping {
                        value.add_label(label);
                    }
                }
                ConfigData::Sequence(sequence) => {
                    for ref mut value in sequence {
                        value.add_label(label);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn select_label(&self, label: &str) -> Option<ConfigValue> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    let mut new_mapping = HashMap::new();
                    for (key, value) in mapping {
                        let new_value = value.select_label(label);
                        if !new_value.is_none() {
                            new_mapping.insert(key.to_owned(), new_value.unwrap());
                        }
                    }
                    if !new_mapping.is_empty() {
                        return Some(ConfigValue {
                            source: self.source.clone(),
                            labels: self.labels.clone(),
                            value: Some(Box::new(ConfigData::Mapping(new_mapping))),
                        });
                    }
                }
                ConfigData::Sequence(sequence) => {
                    let mut new_sequence = Vec::new();
                    for value in sequence {
                        let new_value = value.select_label(label);
                        if !new_value.is_none() {
                            new_sequence.push(new_value.unwrap());
                        }
                    }
                    if !new_sequence.is_empty() {
                        return Some(ConfigValue {
                            source: self.source.clone(),
                            labels: self.labels.clone(),
                            value: Some(Box::new(ConfigData::Sequence(new_sequence))),
                        });
                    }
                }
                ConfigData::Value(_) => {
                    if self.labels.contains(&label.to_string()) {
                        return Some(self.clone());
                    }
                }
            }
        }
        None
    }

    pub fn dig(&self, keypath: Vec<&str>) -> Option<ConfigValue> {
        let mut keypath = keypath.to_owned();
        let key = if keypath.len() > 0 {
            keypath.remove(0)
        } else {
            return Some(self.clone());
        };
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    if let Some(value) = mapping.get(key) {
                        if keypath.is_empty() {
                            return Some(value.clone());
                        } else {
                            return value.dig(keypath);
                        }
                    }
                }
                ConfigData::Sequence(sequence) => {
                    if let Ok(index) = key.parse::<usize>() {
                        if let Some(value) = sequence.get(index) {
                            if keypath.is_empty() {
                                return Some(value.clone());
                            } else {
                                return value.dig(keypath);
                            }
                        }
                    }
                }
                ConfigData::Value(_) => {}
            }
        }
        None
    }

    pub fn dig_mut(&mut self, keypath: Vec<&str>) -> Option<&mut ConfigValue> {
        let mut keypath = keypath.to_owned();
        let key = if keypath.len() > 0 {
            keypath.remove(0)
        } else {
            return Some(self);
        };

        if let Some(data) = self.value.as_mut().map(|data| data.as_mut()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    if let Some(value) = mapping.get_mut(key) {
                        if keypath.is_empty() {
                            return Some(value);
                        } else {
                            return value.dig_mut(keypath);
                        }
                    }
                }
                ConfigData::Sequence(sequence) => {
                    if let Ok(index) = key.parse::<usize>() {
                        if let Some(value) = sequence.get_mut(index) {
                            if keypath.is_empty() {
                                return Some(value);
                            } else {
                                return value.dig_mut(keypath);
                            }
                        }
                    }
                }
                ConfigData::Value(_) => {}
            }
        }

        None
    }

    pub fn is_str(&self) -> bool {
        self.as_str().is_some()
    }

    pub fn as_str(&self) -> Option<String> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Value(value) => {
                    if let Some(value) = value.as_str() {
                        return Some(value.to_string());
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn as_str_forced(&self) -> Option<String> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            if let ConfigData::Value(value) = data {
                match value {
                    serde_yaml::Value::Null => return None,
                    serde_yaml::Value::Bool(value) => return Some(value.to_string()),
                    serde_yaml::Value::String(value) => return Some(value.to_string()),
                    serde_yaml::Value::Number(value) => return Some(value.to_string()),
                    serde_yaml::Value::Sequence(_) => return None,
                    serde_yaml::Value::Mapping(_) => return None,
                    serde_yaml::Value::Tagged(_) => return None,
                }
            }
        }
        None
    }

    pub fn as_str_mut(&mut self) -> Option<&mut String> {
        if let Some(data) = self.value.as_mut().map(|data| data.as_mut()) {
            if let ConfigData::Value(value) = data {
                if let serde_yaml::Value::String(value) = value {
                    return Some(value);
                }
            }
        }
        None
    }

    #[allow(dead_code)]
    pub fn is_bool(&self) -> bool {
        self.as_bool().is_some()
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Value(value) => {
                    if let Some(value) = value.as_bool() {
                        return Some(value);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[allow(dead_code)]
    pub fn is_float(&self) -> bool {
        self.as_float().is_some()
    }

    pub fn as_float(&self) -> Option<f64> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Value(value) => {
                    if let Some(value) = value.as_f64() {
                        return Some(value);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[allow(dead_code)]
    pub fn is_integer(&self) -> bool {
        self.as_integer().is_some()
    }

    pub fn as_integer(&self) -> Option<i64> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Value(value) => {
                    if let Some(value) = value.as_i64() {
                        return Some(value);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn as_unsigned_integer(&self) -> Option<u64> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Value(value) => {
                    if let Some(value) = value.as_u64() {
                        return Some(value);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn is_array(&self) -> bool {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Sequence(_) => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    pub fn as_array(&self) -> Option<Vec<ConfigValue>> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Sequence(sequence) => {
                    let mut new_sequence = Vec::new();
                    for value in sequence {
                        new_sequence.push(value.clone());
                    }
                    return Some(new_sequence);
                }
                _ => {}
            }
        }
        None
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<ConfigValue>> {
        if let Some(data) = self.value.as_mut().map(|data| data.as_mut()) {
            if let ConfigData::Sequence(sequence) = data {
                return Some(sequence);
            }
        }
        None
    }

    pub fn is_table(&self) -> bool {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(_) => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    pub fn as_table(&self) -> Option<HashMap<String, ConfigValue>> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    let mut new_mapping = HashMap::new();
                    for (key, value) in mapping {
                        new_mapping.insert(key.to_string(), value.clone());
                    }
                    return Some(new_mapping);
                }
                _ => {}
            }
        }
        None
    }

    pub fn as_table_mut(&mut self) -> Option<&mut HashMap<String, ConfigValue>> {
        if let Some(data) = self.value.as_mut().map(|data| data.as_mut()) {
            if let ConfigData::Mapping(mapping) = data {
                return Some(mapping);
            }
        }
        None
    }

    pub fn select_keys(&self, keys: Vec<String>) -> Option<ConfigValue> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    let mut new_mapping = HashMap::new();
                    for key in keys {
                        if let Some(value) = mapping.get(&key) {
                            new_mapping.insert(key, value.clone());
                        }
                    }
                    return Some(ConfigValue {
                        value: Some(Box::new(ConfigData::Mapping(new_mapping))),
                        labels: self.labels.clone(),
                        source: self.source.clone(),
                    });
                }
                _ => {}
            }
        }
        None
    }

    pub fn get(&self, key: &str) -> Option<ConfigValue> {
        match self.dig(vec![key]) {
            Some(config_value) => {
                if let Some(data) = config_value.value.as_ref().map(|data| data.as_ref()) {
                    match data {
                        ConfigData::Value(value) => {
                            if value.is_null() {
                                return None;
                            }
                        }
                        _ => {}
                    }
                }
                return Some(config_value);
            }
            None => {
                return None;
            }
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut ConfigValue> {
        self.dig_mut(vec![key])
    }

    pub fn get_as_str(&self, key: &str) -> Option<String> {
        if let Some(value) = self.get(key) {
            return value.as_str();
        }
        None
    }

    pub fn get_as_str_forced(&self, key: &str) -> Option<String> {
        if let Some(value) = self.get(key) {
            return value.as_str_forced();
        }
        None
    }

    pub fn get_as_bool(&self, key: &str) -> Option<bool> {
        if let Some(value) = self.get(key) {
            return value.as_bool();
        }
        None
    }

    pub fn get_as_float(&self, key: &str) -> Option<f64> {
        if let Some(value) = self.get(key) {
            return value.as_float();
        }
        None
    }

    #[allow(dead_code)]
    pub fn get_as_integer(&self, key: &str) -> Option<i64> {
        if let Some(value) = self.get(key) {
            return value.as_integer();
        }
        None
    }

    pub fn get_as_unsigned_integer(&self, key: &str) -> Option<u64> {
        if let Some(value) = self.get(key) {
            return value.as_unsigned_integer();
        }
        None
    }

    pub fn get_as_array(&self, key: &str) -> Option<Vec<ConfigValue>> {
        if let Some(value) = self.get(key) {
            return value.as_array();
        }
        None
    }

    #[allow(dead_code)]
    pub fn get_as_array_mut(&mut self, key: &str) -> Option<&mut Vec<ConfigValue>> {
        if let Some(&mut ref mut value) = self.get_mut(key) {
            return value.as_array_mut();
        }
        None
    }

    pub fn get_as_table(&self, key: &str) -> Option<HashMap<String, ConfigValue>> {
        if let Some(value) = self.get(key) {
            return value.as_table();
        }
        None
    }

    pub fn get_as_table_mut(&mut self, key: &str) -> Option<&mut HashMap<String, ConfigValue>> {
        if let Some(&mut ref mut value) = self.get_mut(key) {
            return value.as_table_mut();
        }
        None
    }

    pub fn unwrap(&self) -> serde_yaml::Value {
        match self.value.as_ref().map(|data| data.as_ref()) {
            Some(ConfigData::Mapping(mapping)) => {
                let mut new_mapping = HashMap::new();
                for (key, value) in mapping {
                    new_mapping.insert(key.to_owned(), value.unwrap().clone());
                }
                return serde_yaml::to_value(new_mapping).unwrap();
            }
            Some(ConfigData::Sequence(sequence)) => {
                let mut new_sequence = Vec::new();
                for value in sequence {
                    new_sequence.push(value.unwrap().clone());
                }
                return serde_yaml::to_value(new_sequence).unwrap();
            }
            Some(ConfigData::Value(value)) => {
                return value.clone();
            }
            None => {
                return serde_yaml::Value::Null;
            }
        }
    }

    pub fn extend(
        &mut self,
        other: ConfigValue,
        strategy: ConfigExtendStrategy,
        keypath: Vec<String>,
    ) {
        if strategy == ConfigExtendStrategy::Keep && !self.is_none_or_empty() {
            return;
        }

        if let (Some(self_value), Some(other_value)) = (&mut self.value, other.value) {
            let _cloned_self_value = self_value.clone();
            let _cloned_other_value = other_value.clone();
            match (&mut **self_value, *other_value) {
                (ConfigData::Mapping(self_mapping), ConfigData::Mapping(other_mapping)) => {
                    for (orig_key, value) in other_mapping {
                        let mut key = orig_key.to_owned();
                        let children_strategy =
                            ConfigValue::key_strategy(&mut key, &keypath, &strategy);

                        let mut keypath = keypath.clone();
                        keypath.push(key.clone());

                        if let Some(self_value) = self_mapping.get_mut(&key) {
                            self_value.extend(value, children_strategy, keypath);
                        } else {
                            let mut new_value =
                                ConfigValue::new_null(other.source.clone(), other.labels.clone());
                            new_value.extend(value, children_strategy, keypath);
                            self_mapping.insert(key, new_value);
                        }
                    }
                }
                (ConfigData::Sequence(self_sequence), ConfigData::Sequence(other_sequence)) => {
                    if strategy == ConfigExtendStrategy::Keep && !self_sequence.is_empty() {
                        return;
                    }

                    let init_index = if strategy == ConfigExtendStrategy::Append {
                        self_sequence.len()
                    } else {
                        0
                    };

                    let mut new_sequence = Vec::new();
                    let children_strategy =
                        ConfigValue::key_strategy(&mut "".to_string(), &keypath, &strategy);
                    for (index, value) in other_sequence.iter().enumerate() {
                        let mut keypath = keypath.clone();
                        keypath.push((init_index + index).to_string());

                        let mut new_value =
                            ConfigValue::new_null(other.source.clone(), other.labels.clone());
                        new_value.extend(value.clone(), children_strategy.clone(), keypath);

                        new_sequence.push(new_value);
                    }

                    match strategy {
                        ConfigExtendStrategy::Append => {
                            'outer: for new_value in new_sequence {
                                let new_value_serde_yaml = new_value.as_serde_yaml();
                                for old_value in self_sequence.iter_mut() {
                                    let old_value_serde_yaml = old_value.as_serde_yaml();
                                    if old_value_serde_yaml == new_value_serde_yaml {
                                        continue 'outer;
                                    }
                                }
                                self_sequence.push(new_value);
                            }
                        }
                        ConfigExtendStrategy::Prepend => {
                            'outer: for old_value in self_sequence.iter_mut() {
                                let old_value_serde_yaml = old_value.as_serde_yaml();
                                for new_value in new_sequence.iter() {
                                    let new_value_serde_yaml = new_value.as_serde_yaml();
                                    if old_value_serde_yaml == new_value_serde_yaml {
                                        continue 'outer;
                                    }
                                }
                                new_sequence.push(old_value.clone());
                            }
                            *self_sequence = new_sequence;
                        }
                        _ => {
                            *self_sequence = new_sequence;
                        }
                    }
                }
                (ConfigData::Value(self_null), ConfigData::Mapping(other_mapping))
                    if self_null.is_null() || strategy != ConfigExtendStrategy::Keep =>
                {
                    let mut new_mapping = HashMap::new();
                    for (orig_key, value) in other_mapping {
                        let mut key = orig_key.to_owned();
                        let children_strategy =
                            ConfigValue::key_strategy(&mut key, &keypath, &strategy);

                        let mut keypath = keypath.clone();
                        keypath.push(key.clone());

                        let mut new_value =
                            ConfigValue::new_null(other.source.clone(), other.labels.clone());
                        new_value.extend(value, children_strategy, keypath);
                        new_mapping.insert(key, new_value);
                    }
                    *self_value = Box::new(ConfigData::Mapping(new_mapping));
                }
                (ConfigData::Value(self_null), ConfigData::Sequence(other_sequence))
                    if self_null.is_null() || strategy != ConfigExtendStrategy::Keep =>
                {
                    let mut new_sequence = Vec::new();
                    let children_strategy =
                        ConfigValue::key_strategy(&mut "".to_string(), &keypath, &strategy);
                    for (index, value) in other_sequence.iter().enumerate() {
                        let mut keypath = keypath.clone();
                        keypath.push(index.to_string());

                        let mut new_value =
                            ConfigValue::new_null(other.source.clone(), other.labels.clone());
                        new_value.extend(value.clone(), children_strategy.clone(), keypath);

                        new_sequence.push(new_value);
                    }
                    *self_value = Box::new(ConfigData::Sequence(new_sequence));
                }
                (ConfigData::Value(self_null), ConfigData::Value(other_val))
                    if self_null.is_null() || strategy != ConfigExtendStrategy::Keep =>
                {
                    self.source = other.source.clone();
                    self.labels.clear();
                    self.labels.extend(other.labels.clone());
                    *self_value = Box::new(ConfigData::Value(other_val));
                    self.transform(&keypath);
                }
                _ => {
                    // Nothing to do
                }
            }
        } else {
            omni_error!("error parsing configuration files");
        }
    }

    fn key_strategy(
        key: &mut String,
        keypath: &Vec<String>,
        strategy: &ConfigExtendStrategy,
    ) -> ConfigExtendStrategy {
        if *strategy == ConfigExtendStrategy::Raw || (keypath.is_empty() && key == "suggest_config")
        {
            return ConfigExtendStrategy::Raw;
        }

        if *keypath == vec!["path".to_string()] {
            if key == "append" {
                return ConfigExtendStrategy::Append;
            } else if key == "prepend" {
                return ConfigExtendStrategy::Prepend;
            }
        }

        if key.ends_with("__toappend") {
            *key = key.strip_suffix("__toappend").unwrap().to_owned();
            return ConfigExtendStrategy::Append;
        } else if key.ends_with("__toprepend") {
            *key = key.strip_suffix("__toprepend").unwrap().to_owned();
            return ConfigExtendStrategy::Prepend;
        } else if key.ends_with("__toreplace") {
            *key = key.strip_suffix("__toreplace").unwrap().to_owned();
            return ConfigExtendStrategy::Replace;
        } else if key.ends_with("__ifnone") {
            *key = key.strip_suffix("__ifnone").unwrap().to_owned();
            return ConfigExtendStrategy::Keep;
        }

        ConfigExtendStrategy::Default
    }

    fn transform(&mut self, keypath: &Vec<String>) {
        if (keypath.len() == 3
            && ((keypath[0] == "path" && ["append", "prepend"].contains(&keypath[1].as_str()))
                || (keypath[0] == "org" && keypath[2] == "worktree")))
            || (keypath.len() == 1 && keypath[0] == "worktree")
        {
            if let Some(data) = self.value.as_mut().map(|data| data.as_mut()) {
                if let ConfigData::Value(value) = data {
                    if let serde_yaml::Value::String(string_value) = value {
                        let value_string = string_value.to_owned();
                        let mut abs_path = value_string.clone();
                        if abs_path.starts_with("~/") {
                            abs_path = Path::new(&*HOME)
                                .join(abs_path.trim_start_matches("~/"))
                                .to_str()
                                .unwrap()
                                .to_string();
                        }
                        if !abs_path.starts_with("/") {
                            if let ConfigSource::File(source) = self.source.clone() {
                                if let Some(source) = Path::new(&source).parent() {
                                    abs_path = source.join(abs_path).to_str().unwrap().to_string();
                                }
                            }
                        }
                        *value = serde_yaml::Value::String(abs_path);
                    }
                }
            }
        }
    }

    fn is_none_or_empty(&self) -> bool {
        self.value.is_none() || self.is_value_empty()
    }

    fn is_value_empty(&self) -> bool {
        if let Some(ref value) = self.value {
            match **value {
                ConfigData::Mapping(ref mapping) => mapping.is_empty(),
                ConfigData::Sequence(ref sequence) => sequence.is_empty(),
                _ => false,
            }
        } else {
            true
        }
    }

    pub fn get_source(&self) -> &ConfigSource {
        &self.source
    }

    pub fn as_serde_yaml(&self) -> serde_yaml::Value {
        if let Some(ref value) = self.value {
            match **value {
                ConfigData::Mapping(ref mapping) => {
                    let mut serde_mapping = serde_yaml::Mapping::new();
                    for (key, value) in mapping {
                        serde_mapping.insert(
                            serde_yaml::Value::String(key.to_owned()),
                            value.as_serde_yaml(),
                        );
                    }
                    serde_yaml::Value::Mapping(serde_mapping)
                }
                ConfigData::Sequence(ref sequence) => {
                    let mut serde_sequence = serde_yaml::Sequence::new();
                    for value in sequence {
                        serde_sequence.push(value.as_serde_yaml());
                    }
                    serde_yaml::Value::Sequence(serde_sequence)
                }
                ConfigData::Value(ref value) => value.to_owned(),
            }
        } else {
            serde_yaml::Value::Null
        }
    }

    pub fn as_yaml(&self) -> String {
        let serde_yaml = self.as_serde_yaml();
        let serde_yaml = sort_serde_yaml(&serde_yaml);
        serde_yaml::to_string(&serde_yaml).unwrap()
    }

    #[allow(dead_code)]
    pub fn set_value(&mut self, value: Option<Box<ConfigData>>) {
        self.value = value;
    }
}

fn sort_serde_yaml(value: &serde_yaml::Value) -> serde_yaml::Value {
    match value {
        serde_yaml::Value::Sequence(seq) => {
            let sorted_seq: Vec<serde_yaml::Value> =
                seq.iter().map(|v| sort_serde_yaml(v)).collect();
            serde_yaml::Value::Sequence(sorted_seq)
        }
        serde_yaml::Value::Mapping(mapping) => {
            let sorted_mapping: BTreeMap<String, serde_yaml::Value> = mapping
                .iter()
                .map(|(k, v)| (k.as_str().unwrap().to_owned(), sort_serde_yaml(v)))
                .collect();
            let sorted_mapping: serde_yaml::Mapping = sorted_mapping
                .into_iter()
                .map(|(k, v)| (serde_yaml::Value::String(k), v))
                .collect();
            serde_yaml::Value::Mapping(sorted_mapping)
        }
        _ => value.clone(),
    }
}
