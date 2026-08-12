use anchor_lang::prelude::*;

// ================================ OFT Events ================================
// EVM alignment: IOFT.sol
//
// Every event carries an `oft_store` pubkey: a single OFT program manages many
// independent OFTStore instances (one per OFT deployment, mirroring how the
// SPL Token program manages many `Mint` accounts), so events carry the store
// pubkey to let off-chain consumers attribute each event to the right instance.
//
// In OFTSent / OFTReceived, `oft_store` is appended LAST (unlike newer events
// where it is the first field). Defensive choice: these two events predated
// the field, and appending preserves the existing prefix layout in case any
// decoder relies on field order. Do not reorder.

#[event]
pub struct OFTSent {
    pub guid: [u8; 32],
    pub dst_eid: u32,
    // `from` is the token account. `sender` is the signer/authority at send time.
    // Token account authority can change later, so `sender` gives indexers a stable initiator
    // record.
    pub from: Pubkey,
    pub amount_sent_ld: u64,
    pub amount_received_ld: u64,
    pub sender: Pubkey,
    pub oft_store: Pubkey,
}

#[event]
pub struct OFTReceived {
    pub guid: [u8; 32],
    pub src_eid: u32,
    pub to: Pubkey,
    pub amount_received_ld: u64,
    pub oft_store: Pubkey,
}

// ================================ ALT Events ================================

#[event]
pub struct AddressLookupTablesSet {
    pub oft_store: Pubkey,
    pub alts: Vec<Pubkey>,
}
