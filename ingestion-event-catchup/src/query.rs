use serde::{Deserialize, Serialize};

pub fn get_query() -> Request {
    let query = "
query Test {
    eventByContractId(searchedContractId: \"CAUEYBG456425X627TP7JGLZTJOGYSH3XBDKNBTPUXOFIVVYYQ3UTHFR\") {
        nodes {
        txInfoByTx {
            ledgerByLedger {
            closeTime,
            sequence
            }
        }
        contractId,
        topic1,
        topic2,
        topic3,
        topic4,
        data
        }
    }
}
    ".to_string();

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
