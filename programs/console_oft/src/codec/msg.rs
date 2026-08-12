use crate::OFTError;
use anchor_lang::prelude::{require, Pubkey, Result};

// EVM alignment: OFTMsgCodec
// OFT send message = [send_to(32)][amount_sd(8)]([sender(32)][compose_msg])
// The [sender][compose_msg] tail is present only for SEND_AND_CALL messages.
const SEND_TO_OFFSET: usize = 0;
const SEND_AMOUNT_SD_OFFSET: usize = 32;
const SENDER_OFFSET: usize = 40;
const COMPOSE_MSG_OFFSET: usize = 72;

pub fn encode(
    send_to: [u8; 32],
    amount_sd: u64,
    sender: Pubkey,
    compose_msg: Option<&[u8]>,
) -> Vec<u8> {
    if let Some(msg) = compose_msg {
        let mut encoded = Vec::with_capacity(COMPOSE_MSG_OFFSET + msg.len()); // 32 + 8 + 32
        encoded.extend_from_slice(&send_to);
        encoded.extend_from_slice(&amount_sd.to_be_bytes());
        encoded.extend_from_slice(sender.to_bytes().as_ref());
        encoded.extend_from_slice(msg);
        encoded
    } else {
        let mut encoded = Vec::with_capacity(40); // 32 + 8
        encoded.extend_from_slice(&send_to);
        encoded.extend_from_slice(&amount_sd.to_be_bytes());
        encoded
    }
}

pub fn send_to(message: &[u8]) -> Result<[u8; 32]> {
    validate_message(message)?;
    let mut send_to = [0; 32];
    send_to.copy_from_slice(&message[SEND_TO_OFFSET..SEND_AMOUNT_SD_OFFSET]);
    Ok(send_to)
}

pub fn amount_sd(message: &[u8]) -> Result<u64> {
    validate_message(message)?;
    let mut amount_sd_bytes = [0; 8];
    amount_sd_bytes.copy_from_slice(&message[SEND_AMOUNT_SD_OFFSET..SENDER_OFFSET]);
    Ok(u64::from_be_bytes(amount_sd_bytes))
}

/// Compose tail `[sender][compose_msg]` (EVM `OFTMsgCodec.composeMsg`), or `None`
/// for a plain SEND message. The `_with_sender` suffix makes the sender prefix
/// explicit; EVM returns the same bytes under the bare `composeMsg` name.
pub fn compose_msg_with_sender(message: &[u8]) -> Result<Option<&[u8]>> {
    validate_message(message)?;
    if message.len() > SENDER_OFFSET {
        Ok(Some(&message[SENDER_OFFSET..]))
    } else {
        Ok(None)
    }
}

fn validate_message(message: &[u8]) -> Result<()> {
    require!(message.len() >= SENDER_OFFSET, OFTError::InvalidMessage);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_send_to() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for (i, slot) in bytes.iter_mut().enumerate() {
            *slot = i as u8;
        }
        bytes
    }

    fn sample_sender() -> Pubkey {
        Pubkey::new_from_array([0xAB; 32])
    }

    #[test]
    fn encode_without_compose_msg_has_expected_length_and_layout() {
        let send_to = sample_send_to();
        let encoded = encode(send_to, 0x0102_0304_0506_0708, sample_sender(), None);

        assert_eq!(encoded.len(), 40);
        assert_eq!(&encoded[0..32], &send_to);
        // amount_sd is encoded big-endian
        assert_eq!(&encoded[32..40], &0x0102_0304_0506_0708u64.to_be_bytes());
    }

    #[test]
    fn encode_with_compose_msg_appends_sender_then_message() {
        let send_to = sample_send_to();
        let sender = sample_sender();
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF];

        let encoded = encode(send_to, 42, sender, Some(&payload));

        assert_eq!(encoded.len(), 72 + payload.len());
        assert_eq!(&encoded[0..32], &send_to);
        assert_eq!(&encoded[32..40], &42u64.to_be_bytes());
        assert_eq!(&encoded[40..72], sender.to_bytes().as_ref());
        assert_eq!(&encoded[72..], &payload[..]);
    }

    #[test]
    fn round_trip_without_compose_msg() {
        let send_to_in = sample_send_to();
        let amount_in = 1_234_567_890u64;

        let encoded = encode(send_to_in, amount_in, sample_sender(), None);

        assert_eq!(send_to(&encoded).unwrap(), send_to_in);
        assert_eq!(amount_sd(&encoded).unwrap(), amount_in);
        assert_eq!(compose_msg_with_sender(&encoded).unwrap(), None);
    }

    #[test]
    fn round_trip_with_compose_msg() {
        let send_to_in = sample_send_to();
        let sender = sample_sender();
        let amount_in = u64::MAX;
        let payload = vec![1, 2, 3, 4, 5];

        let encoded = encode(send_to_in, amount_in, sender, Some(&payload));

        assert_eq!(send_to(&encoded).unwrap(), send_to_in);
        assert_eq!(amount_sd(&encoded).unwrap(), amount_in);
        // The decoded compose_msg slice retains the sender prefix followed by the raw payload —
        // splitting that prefix is the receiver's responsibility.
        let decoded = compose_msg_with_sender(&encoded).unwrap().expect("compose_msg present");
        assert_eq!(&decoded[0..32], sender.to_bytes().as_ref());
        assert_eq!(&decoded[32..], &payload[..]);
    }

    #[test]
    fn compose_msg_boundary_at_offset() {
        // Exactly SENDER_OFFSET (40) bytes => no compose payload.
        let encoded = encode(sample_send_to(), 0, sample_sender(), None);
        assert_eq!(encoded.len(), SENDER_OFFSET);
        assert_eq!(compose_msg_with_sender(&encoded).unwrap(), None);

        // One extra byte => Some(...) with that byte.
        let mut with_extra = encoded.clone();
        with_extra.push(0x99);
        assert_eq!(compose_msg_with_sender(&with_extra).unwrap(), Some(&[0x99][..]));
    }

    #[test]
    fn decode_rejects_short_message() {
        let short = [0u8; SENDER_OFFSET - 1];
        assert!(send_to(&short).is_err());
        assert!(amount_sd(&short).is_err());
        assert!(compose_msg_with_sender(&short).is_err());
    }
}
