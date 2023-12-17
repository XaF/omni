use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::identities::Zero;

const BASE: usize = 62;
const BASE62_CHARS: [char; BASE] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I',
    'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'a', 'b',
    'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u',
    'v', 'w', 'x', 'y', 'z',
];

pub fn encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "".to_string();
    }

    let mut input = vec![1u8];
    input.extend_from_slice(bytes);

    let base = BigUint::from(BASE.to_owned() as u64);
    let zero = BigUint::zero();

    let mut encoded = String::new();
    let mut value = BigUint::from_bytes_be(&input);
    while value > zero {
        let (div, rem) = value.div_rem(&base);
        encoded.push(BASE62_CHARS[rem.try_into().unwrap_or(0)]);
        value = div;
    }

    encoded
}
