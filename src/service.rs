// Copyright 2018 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use exonum::blockchain::{Service, Transaction, TransactionSet};
use exonum::crypto::Hash;
use exonum::encoding::Error as EncodingError;
use exonum::messages::RawMessage;
use exonum::storage::{Fork, Snapshot};

use blockchain::{BtcAnchoringSchema, Transactions};
use config::GlobalConfig;
use rpc::BtcRelay;
use serde_json;

/// Anchoring service id.
pub const BTC_ANCHORING_SERVICE_ID: u16 = 3;
/// Anchoring service name.
pub const BTC_ANCHORING_SERVICE_NAME: &str = "btc_anchoring";

pub struct BtcAnchoringService {
    pub global_config: GlobalConfig,
    pub btc_relay: Option<Box<BtcRelay>>,
}

impl ::std::fmt::Debug for BtcAnchoringService {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct("BtcAnchoringService").finish()
    }
}

impl Service for BtcAnchoringService {
    fn service_id(&self) -> u16 {
        BTC_ANCHORING_SERVICE_ID
    }

    fn service_name(&self) -> &'static str {
        BTC_ANCHORING_SERVICE_NAME
    }

    fn state_hash(&self, snapshot: &Snapshot) -> Vec<Hash> {
        BtcAnchoringSchema::new(snapshot).state_hash()
    }

    fn tx_from_raw(&self, raw: RawMessage) -> Result<Box<Transaction>, EncodingError> {
        let tx = Transactions::tx_from_raw(raw)?;
        Ok(tx.into())
    }

    fn initialize(&self, _fork: &mut Fork) -> serde_json::Value {
        json!(self.global_config)
    }
}
