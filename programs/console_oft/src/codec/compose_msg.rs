use crate::OFTError;
use anchor_lang::prelude::{require, Result};

// Solana compose messages keep the same field order as EVM's OFTComposeMsgCodec,
// but use a compact u64 amount_ld instead of EVM's uint256 amountLD.
// Layout: [nonce:8][src_eid:4][amount_ld:8][compose_from:32][compose_msg]
const NONCE_OFFSET: usize = 0;
const SRC_EID_OFFSET: usize = 8;
const AMOUNT_LD_OFFSET: usize = 12;
const COMPOSE_FROM_OFFSET: usize = 20;
const COMPOSE_MSG_OFFSET: usize = 52;

pub fn encode(
    nonce: u64,
    src_eid: u32,
    amount_ld: u64,
    compose_msg: &[u8], // [composeFrom][composeMsg]
) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(20 + compose_msg.len()); // 8 + 4 + 8
    encoded.extend_from_slice(&nonce.to_be_bytes());
    encoded.extend_from_slice(&src_eid.to_be_bytes());
    encoded.extend_from_slice(&amount_ld.to_be_bytes());
    encoded.extend_from_slice(compose_msg);
    encoded
}

pub fn nonce(message: &[u8]) -> Result<u64> {
    validate_compose_message(message)?;
    let mut nonce_bytes = [0; 8];
    nonce_bytes.copy_from_slice(&message[NONCE_OFFSET..SRC_EID_OFFSET]);
    Ok(u64::from_be_bytes(nonce_bytes))
}

pub fn src_eid(message: &[u8]) -> Result<u32> {
    validate_compose_message(message)?;
    let mut src_eid_bytes = [0; 4];
    src_eid_bytes.copy_from_slice(&message[SRC_EID_OFFSET..AMOUNT_LD_OFFSET]);
    Ok(u32::from_be_bytes(src_eid_bytes))
}

pub fn amount_ld(message: &[u8]) -> Result<u64> {
    validate_compose_message(message)?;
    let mut amount_ld_bytes = [0; 8];
    amount_ld_bytes.copy_from_slice(&message[AMOUNT_LD_OFFSET..COMPOSE_FROM_OFFSET]);
    Ok(u64::from_be_bytes(amount_ld_bytes))
}

pub fn compose_from(message: &[u8]) -> Result<[u8; 32]> {
    validate_compose_message(message)?;
    let mut compose_from = [0; 32];
    compose_from.copy_from_slice(&message[COMPOSE_FROM_OFFSET..COMPOSE_MSG_OFFSET]);
    Ok(compose_from)
}

pub fn compose_msg(message: &[u8]) -> Result<&[u8]> {
    validate_compose_message(message)?;
    Ok(&message[COMPOSE_MSG_OFFSET..])
}

fn validate_compose_message(message: &[u8]) -> Result<()> {
    require!(message.len() >= COMPOSE_MSG_OFFSET, OFTError::InvalidMessage);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_compose_from() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for (i, slot) in bytes.iter_mut().enumerate() {
            *slot = (i as u8).wrapping_add(0x10);
        }
        bytes
    }

    /// Builds the [composeFrom][composeMsg] tail expected by `encode`.
    fn build_tail(compose_from: &[u8; 32], payload: &[u8]) -> Vec<u8> {
        let mut tail = Vec::with_capacity(32 + payload.len());
        tail.extend_from_slice(compose_from);
        tail.extend_from_slice(payload);
        tail
    }

    #[test]
    fn encode_layout_is_big_endian_and_concatenated() {
        let cf = sample_compose_from();
        let payload = vec![0xCA, 0xFE];
        let tail = build_tail(&cf, &payload);

        let encoded = encode(0x0102_0304_0506_0708, 0x0A0B_0C0D, 0x1112_1314_1516_1718, &tail);

        // header is 8 + 4 + 8 = 20 bytes, then 32 bytes compose_from, then payload.
        assert_eq!(encoded.len(), 20 + 32 + payload.len());
        assert_eq!(&encoded[0..8], &0x0102_0304_0506_0708u64.to_be_bytes());
        assert_eq!(&encoded[8..12], &0x0A0B_0C0Du32.to_be_bytes());
        assert_eq!(&encoded[12..20], &0x1112_1314_1516_1718u64.to_be_bytes());
        assert_eq!(&encoded[20..52], &cf);
        assert_eq!(&encoded[52..], &payload[..]);
    }

    #[test]
    fn round_trip_with_payload() {
        let nonce_in = 7u64;
        let src_eid_in = 30_101u32;
        let amount_in = 1_000_000_000u64;
        let cf = sample_compose_from();
        let payload = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let tail = build_tail(&cf, &payload);

        let encoded = encode(nonce_in, src_eid_in, amount_in, &tail);

        assert_eq!(nonce(&encoded).unwrap(), nonce_in);
        assert_eq!(src_eid(&encoded).unwrap(), src_eid_in);
        assert_eq!(amount_ld(&encoded).unwrap(), amount_in);
        assert_eq!(compose_from(&encoded).unwrap(), cf);
        assert_eq!(compose_msg(&encoded).unwrap(), payload.as_slice());
    }

    #[test]
    fn compose_msg_boundary_at_offset() {
        let cf = sample_compose_from();
        // Empty payload => total length is exactly COMPOSE_MSG_OFFSET (52).
        let encoded = encode(0, 0, 0, &build_tail(&cf, &[]));
        assert_eq!(encoded.len(), COMPOSE_MSG_OFFSET);
        assert!(compose_msg(&encoded).unwrap().is_empty());
        assert_eq!(compose_from(&encoded).unwrap(), cf);

        // One extra byte => returned as the compose payload.
        let mut with_extra = encoded.clone();
        with_extra.push(0x77);
        assert_eq!(compose_msg(&with_extra).unwrap(), &[0x77]);
    }

    #[test]
    fn round_trip_with_extreme_values() {
        let cf = sample_compose_from();
        let tail = build_tail(&cf, &[]);

        let encoded = encode(u64::MAX, u32::MAX, u64::MAX, &tail);

        assert_eq!(nonce(&encoded).unwrap(), u64::MAX);
        assert_eq!(src_eid(&encoded).unwrap(), u32::MAX);
        assert_eq!(amount_ld(&encoded).unwrap(), u64::MAX);
        assert_eq!(compose_from(&encoded).unwrap(), cf);
    }

    #[test]
    fn decode_rejects_short_message() {
        let short = [0u8; COMPOSE_MSG_OFFSET - 1];
        assert!(nonce(&short).is_err());
        assert!(src_eid(&short).is_err());
        assert!(amount_ld(&short).is_err());
        assert!(compose_from(&short).is_err());
        assert!(compose_msg(&short).is_err());
    }
}
