//! shortId 生成：8 字符大小写字母数字随机（PRD §5.2）
//!
//! 沿用 byteRef 的标识方案——碰撞概率（62^8 ≈ 2.18×10¹⁴）在单人本地库规模下足够低，
//! 入库前由 service 层做查重并重试。

use rand::Rng;

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
pub const SHORT_ID_LEN: usize = 8;

/// 生成一个 8 字符 random shortId
pub fn generate() -> String {
    let mut rng = rand::rng();
    (0..SHORT_ID_LEN)
        .map(|_| {
            let idx = rng.random_range(0..ALPHABET.len());
            ALPHABET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_correct_length() {
        let id = generate();
        assert_eq!(id.len(), SHORT_ID_LEN);
    }

    #[test]
    fn uses_alphabet_only() {
        for _ in 0..100 {
            let id = generate();
            for c in id.chars() {
                assert!(c.is_ascii_alphanumeric());
            }
        }
    }
}
