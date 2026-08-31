use std::fmt::Write as _;

pub trait HexaString: Sized {
    fn as_hexa_string(&self) -> String;
    fn from_hexa_string(hexa_string: &str) -> Option<Self>;
}

impl HexaString for Vec<u8> {
    fn as_hexa_string(&self) -> String {
        self.iter().fold(String::with_capacity(self.len() * 2), |mut acc, b| {
            write!(acc, "{:02x}", b).unwrap();
            acc
        })
    }

    fn from_hexa_string(hexa_string: &str) -> Option<Self> {
        if hexa_string.len() % 2 != 0 {
            eprintln!("Error: Hexa string must have an even number of characters.");
            return None;
        }

        let chars: Vec<char> = hexa_string.chars().collect();
        let mut bytes = Vec::with_capacity(chars.len() / 2);

        for chunk in chars.chunks(2) {
            let byte_string: String = chunk.iter().collect();
            match u8::from_str_radix(&byte_string, 16) {
                Ok(byte) => bytes.push(byte),
                Err(_) => {
                    eprintln!("Error: Invalid hex character encountered in '{}'.", byte_string);
                    return None;
                }
            }
        }

        Some(bytes)
    }
}
