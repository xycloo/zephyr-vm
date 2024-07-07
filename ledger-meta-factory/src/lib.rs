use ledger::sample_ledger;
use stellar_xdr::next::{
    ContractEvent, ContractEventV0, ExtensionPoint, GeneralizedTransactionSet, Hash,
    InvokeContractArgs, InvokeHostFunctionOp, LedgerCloseMeta, LedgerEntryChanges, Limits,
    Operation, OperationMeta, ReadXdr, ScAddress, ScSymbol, ScVal, SequenceNumber,
    SorobanTransactionMeta, TimePoint, Transaction, TransactionEnvelope,
    TransactionMeta, TransactionMetaV3, TransactionPhase, TransactionResult, TransactionResultExt,
    TransactionResultMeta, TransactionResultPair, TransactionResultResult, TransactionV1Envelope,
    TxSetComponent, TxSetComponentTxsMaybeDiscountedFee, Uint256, WriteXdr,
};

mod ledger;

pub struct TransitionPretty {
    pub inner: Transition,
}

impl TransitionPretty {
    pub fn new() -> Self {
        Self {
            inner: Transition::new(),
        }
    }

    pub fn contract_event(
        &mut self,
        contract: impl ToString,
        topics: Vec<ScVal>,
        data: ScVal,
    ) -> anyhow::Result<ContractEvent> {
        let hash = Hash(stellar_strkey::Contract::from_string(&contract.to_string())?.0);

        let event = ContractEvent {
            ext: ExtensionPoint::V0,
            contract_id: Some(hash),
            type_: stellar_xdr::next::ContractEventType::Contract,
            body: stellar_xdr::next::ContractEventBody::V0(ContractEventV0 {
                topics: topics.try_into().unwrap(),
                data,
            }),
        };

        self.inner.add_soroban_event(event.clone());
        Ok(event)
    }
}

pub struct Transition {
    meta: LedgerCloseMeta,
}

impl Transition {
    pub fn new() -> Self {
        let meta = LedgerCloseMeta::from_xdr_base64(sample_ledger(), Limits::none()).unwrap();
        Self { meta }
    }

    pub fn meta_object(&self) -> LedgerCloseMeta {
        self.meta.clone()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.meta.to_xdr(Limits::none()).unwrap()
    }

    pub fn to_base64(&self) -> String {
        self.meta.to_xdr_base64(Limits::none()).unwrap()
    }

    pub fn set_sequence(&mut self, new_sequence: i64) {
        match self.meta.clone() {
            LedgerCloseMeta::V1(mut v1) => {
                v1.ledger_header.header.ledger_seq = new_sequence as u32;
                self.meta = LedgerCloseMeta::V1(v1)
            }

            LedgerCloseMeta::V0(mut v0) => {
                v0.ledger_header.header.ledger_seq = new_sequence as u32;
                self.meta = LedgerCloseMeta::V0(v0)
            }
        }
    }

    pub fn set_close_time(&mut self, new_close_time: i64) {
        match self.meta.clone() {
            LedgerCloseMeta::V1(mut v1) => {
                v1.ledger_header.header.scp_value.close_time = TimePoint(new_close_time as u64);
                self.meta = LedgerCloseMeta::V1(v1)
            }

            LedgerCloseMeta::V0(mut v0) => {
                v0.ledger_header.header.scp_value.close_time = TimePoint(new_close_time as u64);
                self.meta = LedgerCloseMeta::V0(v0)
            }
        }
    }

    pub fn add_soroban_event(&mut self, event: ContractEvent) {
        self.add_sample_soroban_envelope(event.contract_id.clone().unwrap());

        let txmeta = TransactionResultMeta {
            result: TransactionResultPair {
                transaction_hash: Hash([0; 32]),
                result: TransactionResult {
                    fee_charged: 0,
                    result: TransactionResultResult::TxSuccess(vec![].try_into().unwrap()),
                    ext: TransactionResultExt::V0,
                },
            },
            fee_processing: LedgerEntryChanges(vec![].try_into().unwrap()),
            tx_apply_processing: TransactionMeta::V3(TransactionMetaV3 {
                ext: ExtensionPoint::V0,
                tx_changes_before: LedgerEntryChanges(vec![].try_into().unwrap()),
                tx_changes_after: LedgerEntryChanges(vec![].try_into().unwrap()),
                operations: vec![OperationMeta {
                    changes: LedgerEntryChanges(vec![].try_into().unwrap()),
                }]
                .try_into()
                .unwrap(),
                soroban_meta: Some(SorobanTransactionMeta {
                    ext: ExtensionPoint::V0,
                    return_value: ScVal::Void,
                    diagnostic_events: vec![].try_into().unwrap(),
                    events: vec![event].try_into().unwrap(),
                }),
            }),
        };
        self.processing_append(txmeta);
    }

    fn add_sample_soroban_envelope(&mut self, contract_id: Hash) {
        let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx: Transaction {
                source_account: stellar_xdr::next::MuxedAccount::Ed25519(Uint256([0; 32])),
                fee: 10000,
                seq_num: SequenceNumber(1),
                cond: stellar_xdr::next::Preconditions::None,
                memo: stellar_xdr::next::Memo::None,
                operations: vec![Operation {
                    source_account: None,
                    body: stellar_xdr::next::OperationBody::InvokeHostFunction(
                        InvokeHostFunctionOp {
                            auth: vec![].try_into().unwrap(),
                            host_function: stellar_xdr::next::HostFunction::InvokeContract(
                                InvokeContractArgs {
                                    contract_address: ScAddress::Contract(contract_id),
                                    function_name: ScSymbol("metafactory".try_into().unwrap()),
                                    args: vec![].try_into().unwrap(),
                                },
                            ),
                        },
                    ),
                }]
                .try_into()
                .unwrap(),
                ext: stellar_xdr::next::TransactionExt::V0,
            },
            signatures: vec![].try_into().unwrap(),
        });

        self.set_append(envelope)
    }

    fn set_append(&mut self, tx: TransactionEnvelope) {
        match self.meta.clone() {
            LedgerCloseMeta::V1(mut v1) => {
                let GeneralizedTransactionSet::V1(mut v1_set) = v1.tx_set.clone();

                let TransactionPhase::V0(v0phase) = v1_set.phases[0].clone();
                let v0phase_length = v0phase.len();
                let mut v0phase = v0phase.to_vec();

                let TxSetComponent::TxsetCompTxsMaybeDiscountedFee(
                    TxSetComponentTxsMaybeDiscountedFee { txs, base_fee },
                ) = v0phase.last().unwrap().clone();
                let mut txs = txs.to_vec();
                txs.push(tx);

                v0phase[v0phase_length - 1] = TxSetComponent::TxsetCompTxsMaybeDiscountedFee(
                    TxSetComponentTxsMaybeDiscountedFee {
                        base_fee: base_fee.clone(),
                        txs: txs.try_into().unwrap(),
                    },
                );

                let mut v1_set_phases = v1_set.phases.to_vec();
                v1_set_phases[v1_set.phases.len() - 1] =
                    TransactionPhase::V0(v0phase.try_into().unwrap());

                v1_set.phases = v1_set_phases.try_into().unwrap();

                v1.tx_set = GeneralizedTransactionSet::V1(v1_set);
                self.meta = LedgerCloseMeta::V1(v1)
            }

            LedgerCloseMeta::V0(mut v0) => {
                let mut txs = v0.tx_set.txs.to_vec();
                txs.push(tx);

                v0.tx_set.txs = txs.try_into().unwrap();
                self.meta = LedgerCloseMeta::V0(v0)
            }
        }
    }

    fn processing_append(&mut self, meta: TransactionResultMeta) {
        match self.meta.clone() {
            LedgerCloseMeta::V1(mut v1) => {
                let mut tx_processing = v1.tx_processing.to_vec();
                tx_processing.push(meta);
                v1.tx_processing = tx_processing.try_into().unwrap();

                self.meta = LedgerCloseMeta::V1(v1)
            }

            LedgerCloseMeta::V0(mut v0) => {
                let mut tx_processing = v0.tx_processing.to_vec();
                tx_processing.push(meta);
                v0.tx_processing = tx_processing.try_into().unwrap();

                self.meta = LedgerCloseMeta::V0(v0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use stellar_xdr::next::{ContractEvent, Int128Parts, LedgerCloseMeta, Limits, ScSymbol, ScVal};
    use zephyr_sdk::MetaReader;

    use crate::TransitionPretty;

    fn to_sdk_xdr_lib<F: stellar_xdr::next::WriteXdr, T: soroban_sdk::xdr::ReadXdr>(xdr: F) -> T {
        T::from_xdr(
            xdr.to_xdr(Limits::none()).unwrap(),
            soroban_sdk::xdr::Limits::none(),
        )
        .unwrap()
    }

    #[test]
    fn change_sequence() {
        let mut meta = TransitionPretty::new();
        meta.inner.set_sequence(20000);

        let converted = to_sdk_xdr_lib::<LedgerCloseMeta, soroban_sdk::xdr::LedgerCloseMeta>(
            meta.inner.meta_object(),
        );
        let metareader = MetaReader::new(&converted);
        assert_eq!(20000, metareader.ledger_sequence())
    }

    #[test]
    fn change_timestamp() {
        let mut meta = TransitionPretty::new();
        meta.inner.set_close_time(20000);

        let converted = to_sdk_xdr_lib::<LedgerCloseMeta, soroban_sdk::xdr::LedgerCloseMeta>(
            meta.inner.meta_object(),
        );
        let metareader = MetaReader::new(&converted);
        assert_eq!(20000, metareader.ledger_timestamp())
    }

    #[test]
    fn add_event() {
        let mut meta = TransitionPretty::new();
        let added_event = meta
            .contract_event(
                "CD477X3QMZ76RZORYC6SLMXXRC5OBFGOUAQA7F6NUJMICHJ4DNRKY7ZQ",
                vec![ScVal::Symbol(ScSymbol("transfer".try_into().unwrap()))],
                ScVal::I128(Int128Parts {
                    hi: 0,
                    lo: 2000000000,
                }),
            )
            .unwrap();

        let converted = to_sdk_xdr_lib::<LedgerCloseMeta, soroban_sdk::xdr::LedgerCloseMeta>(
            meta.inner.meta_object(),
        );
        let metareader = MetaReader::new(&converted);

        assert_eq!(
            vec![to_sdk_xdr_lib::<
                ContractEvent,
                soroban_sdk::xdr::ContractEvent,
            >(added_event)],
            metareader.soroban_events()
        );
    }
}
