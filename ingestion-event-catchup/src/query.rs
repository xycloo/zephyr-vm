use serde::{Deserialize, Serialize};

pub fn get_query(contracts: &[String]) -> Request {
    let mut contracts_string = String::from("[");
    for (idx, contract) in contracts.iter().enumerate() {
        if idx == contracts.len() - 1 {
            contracts_string.push_str(&format!("\"{}\"]", contract))
        } else {
            contracts_string.push_str(&format!("\"{}\", ", contract))
        }
    }

    let query = format!("
query Test {{
    eventByContractIds(ids: {contracts_string}) {{
        nodes {{
        txInfoByTx {{
            ledgerByLedger {{
            closeTime,
            sequence
            }}
        }}
        contractId,
        topic1,
        topic2,
        topic3,
        topic4,
        data
        }}
    }}
}}
    ", );

    Request {
        query,
    }
}

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct Vars {
    pubKey: String 
}

#[derive(Serialize, Deserialize)]
pub struct Request {
    query: String,
}


#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Ledger {
    pub closeTime: i64,
    pub sequence: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TxInfo {
    pub ledgerByLedger: Ledger,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EventNode {
    pub txInfoByTx: TxInfo,
    pub contractId: String,
    pub topic1: Option<String>,
    pub topic2: Option<String>,
    pub topic3: Option<String>,
    pub topic4: Option<String>,
    pub data: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EventByContractId {
    pub nodes: Vec<EventNode>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Data {
    pub eventByContractId: EventByContractId,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Response {
    pub data: Data,
}
