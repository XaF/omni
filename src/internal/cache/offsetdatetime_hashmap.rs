use std::collections::HashMap;

use serde::de::Error as deserializer_Error;
use serde::ser::Error as serializer_Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn serialize<S>(map: &HashMap<String, OffsetDateTime>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut result = HashMap::<String, String>::new();
    for (k, v) in map.iter() {
        let string = v.format(&Rfc3339);
        if let Err(err) = string {
            return Err(S::Error::custom(format!(
                "Failed to format OffsetDateTime: {}",
                err
            )));
        }
        result.insert(k.clone(), string.unwrap());
    }
    Ok(result.serialize(serializer)?)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<String, OffsetDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let map = HashMap::<String, String>::deserialize(deserializer)?;
    let mut result = HashMap::<String, OffsetDateTime>::new();
    for (k, v) in map.iter() {
        let odt = OffsetDateTime::parse(v, &Rfc3339);
        if let Err(err) = odt {
            return Err(D::Error::custom(format!(
                "Failed to parse OffsetDateTime: {}",
                err
            )));
        }
        result.insert(k.clone(), odt.unwrap());
    }
    Ok(result)
}
