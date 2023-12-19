use stellar_xdr::next::{LedgerCloseMeta, LedgerEntry, LedgerEntryChange, LedgerKey, TransactionMeta};

#[derive(Clone)]
pub struct EntryChanges {
    pub state: Vec<LedgerEntry>,
    pub removed: Vec<LedgerKey>,
    pub updated: Vec<LedgerEntry>,
    pub created: Vec<LedgerEntry>,
}

pub struct MetaReader<'a>(&'a stellar_xdr::next::LedgerCloseMeta);

impl<'a> MetaReader<'a> {
    pub fn new(meta: &'a LedgerCloseMeta) -> Self {
        Self(meta)
    }

    pub fn ledger_sequence(&self) -> u32 {
        match &self.0 {
            LedgerCloseMeta::V1(v1) => v1.ledger_header.header.ledger_seq,
            LedgerCloseMeta::V0(v0) => v0.ledger_header.header.ledger_seq,
        }
    }

    // todo: add handles for other entries.

    pub fn v1_ledger_entries(&self) -> EntryChanges {
        let mut state_entries = Vec::new();
        let mut removed_entries = Vec::new();
        let mut updated_entries = Vec::new();
        let mut created_entries = Vec::new();

        match &self.0 {
            LedgerCloseMeta::V0(_) => (),
            LedgerCloseMeta::V1(v2) => {
                for tx_processing in v2.tx_processing.iter() {
                    match &tx_processing.tx_apply_processing {
                        TransactionMeta::V3(meta) => {
                            let ops = &meta.operations;

                            for operation in ops.clone().into_vec() {
                                for change in operation.changes.0.iter() {
                                    match &change {
                                        LedgerEntryChange::State(state) => {
                                            state_entries.push(state.clone())
                                        }
                                        LedgerEntryChange::Created(created) => {
                                            created_entries.push(created.clone())
                                        }
                                        LedgerEntryChange::Updated(updated) => {
                                            updated_entries.push(updated.clone())
                                        }
                                        LedgerEntryChange::Removed(removed) => {
                                            removed_entries.push(removed.clone())
                                        }
                                    };
                                }
                            }
                        }
                        _ => (),
                    }
                }
            }
        };

        EntryChanges {
            state: state_entries,
            removed: removed_entries,
            updated: updated_entries,
            created: created_entries,
        }
    }
}
