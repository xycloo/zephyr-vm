use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Ledger {
    pub closeTime: i64,
    pub sequence: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TxInfo {
    pub txHash: String,
    pub ledgerByLedger: Ledger,
    pub resultXdr: Option<String>,
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
pub struct DataAfterLedger {
    #[serde(rename = "eventByContractIdsAndTopics")]
    pub eventByContractIds: EventByContractId,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResponseAfterLedger {
    pub data: DataAfterLedger,
}

pub fn get_query_generic(
    contracts: &[String],
    topic1s: &Vec<String>,
    topic2s: &Vec<String>,
    topic3s: &Vec<String>,
    topic4s: &Vec<String>,
    ledger: Option<i64>,
) -> Request {
    let mut contracts_string = String::from("[");
    for (idx, contract) in contracts.iter().enumerate() {
        if idx == contracts.len() - 1 {
            contracts_string.push_str(&format!("\"{}\"]", contract))
        } else {
            contracts_string.push_str(&format!("\"{}\", ", contract))
        }
    }

    let t1_value = if !topic1s.is_empty() {
        let mut string = String::from(" , t1: [");
        for (idx, topic) in topic1s.iter().enumerate() {
            if idx == topic1s.len() - 1 {
                string.push_str(&format!("\"{topic}\""));
            } else {
                string.push_str(&format!("\"{topic}\", "));
            }
        }
        string.push(']');
        string
    } else {
        String::new()
    };

    let t2_value = if !topic2s.is_empty() {
        let mut string = String::from(" , t2: [");
        for (idx, topic) in topic2s.iter().enumerate() {
            if idx == topic2s.len() - 1 {
                string.push_str(&format!("\"{topic}\""));
            } else {
                string.push_str(&format!("\"{topic}\", "));
            }
        }
        string.push(']');
        string
    } else {
        String::new()
    };

    let t3_value = if !topic3s.is_empty() {
        let mut string = String::from(" , t3: [");
        for (idx, topic) in topic3s.iter().enumerate() {
            if idx == topic3s.len() - 1 {
                string.push_str(&format!("\"{topic}\""));
            } else {
                string.push_str(&format!("\"{topic}\", "));
            }
        }
        string.push(']');
        string
    } else {
        String::new()
    };

    let t4_value = if !topic4s.is_empty() {
        let mut string = String::from(" , t4: [");
        for (idx, topic) in topic4s.iter().enumerate() {
            if idx == topic4s.len() - 1 {
                string.push_str(&format!("\"{topic}\""));
            } else {
                string.push_str(&format!("\"{topic}\", "));
            }
        }
        string.push(']');
        string
    } else {
        String::new()
    };

    let ledger_value = if let Some(ledger) = ledger {
        format!(" , ledgerValue: {ledger}")
    } else {
        Default::default()
    };

    let query = format!(
        "
query Test {{
    eventByContractIdsAndTopics(ids: {contracts_string}{t1_value}{t2_value}{t3_value}{t4_value}{ledger_value}) {{
        nodes {{
        txInfoByTx {{
            txHash,
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
    ",
    );

    Request { query }
}

