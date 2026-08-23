use iroh::SecretKey;

const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const CODE_LEN: usize = 8;

pub fn generate_room_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut raw = String::with_capacity(CODE_LEN);
    for _ in 0..CODE_LEN {
        let i = rng.gen_range(0..ALPHABET.len());
        raw.push(ALPHABET[i] as char);
    }
    format_room_code(&raw)
}

pub fn format_room_code(raw: &str) -> String {
    format!("SNR-{}-{}", &raw[..4], &raw[4..])
}

fn canon_char(c: char) -> Option<char> {
    let c = match c.to_ascii_uppercase() {
        '0' => 'O',
        '1' | 'I' => 'L',
        other => other,
    };
    if ALPHABET.contains(&(c as u8)) {
        Some(c)
    } else {
        None
    }
}

pub fn parse_room_code(input: &str) -> Result<String, String> {
    let alnum: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let upper = alnum.to_ascii_uppercase();
    let body = upper.strip_prefix("SNR").unwrap_or(&upper);
    let n: String = body.chars().filter_map(canon_char).collect();
    if n.len() != CODE_LEN {
        return Err("Use an 8-character room code like SNR-K7MQ-2PLX.".into());
    }
    Ok(format_room_code(&n))
}

pub fn room_secret(formatted_or_raw: &str) -> Result<SecretKey, String> {
    let code = parse_room_code(formatted_or_raw)?;
    let body: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let body = body.strip_prefix("SNR").unwrap_or(&body);
    let hash = blake3::hash(format!("sonora-room-v1:{body}").as_bytes());
    Ok(SecretKey::from_bytes(hash.as_bytes()))
}

pub fn host_id(code: &str) -> Result<iroh::EndpointId, String> {
    Ok(room_secret(code)?.public())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let code = generate_room_code();
        assert!(code.starts_with("SNR-"));
        let parsed = parse_room_code(&code).unwrap();
        assert_eq!(parsed, code);
        let a = room_secret(&code).unwrap();
        let b = room_secret(&code.to_lowercase()).unwrap();
        assert_eq!(a.public(), b.public());
    }
}
