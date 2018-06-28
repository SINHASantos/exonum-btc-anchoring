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

use bitcoin::network::constants::Network;
use blockchain::transactions::Signature;
use blockchain::BtcAnchoringSchema;
use std::env;
use std::ops::{Deref, DerefMut};

use btc;
use exonum::blockchain::Transaction;
use exonum_testkit::{TestKit, TestKitBuilder, TestNetworkConfiguration, TestNode};

use std::sync::{Arc, RwLock};

use btc_transaction_utils::{multisig::RedeemScript, p2wsh, TxInRef};

use config::{GlobalConfig, LocalConfig};
use rand::thread_rng;
use std::collections::HashMap;
use {blockchain::BtcAnchoringState,
     rpc::{BitcoinRpcClient, BitcoinRpcConfig, BtcRelay},
     BtcAnchoringService,
     BTC_ANCHORING_SERVICE_NAME};

pub fn gen_anchoring_config(
    config: &BitcoinRpcConfig,
    network: Network,
    count: u16,
    total_funds: u64,
    anchoring_interval: u64,
) -> (GlobalConfig, Vec<LocalConfig>) {
    let mut rng = thread_rng();
    let count = count as usize;

    let client = BitcoinRpcClient::from(config.clone());
    let (public_keys, private_keys): (Vec<_>, Vec<_>) = (0..count)
        .map(|_| btc::gen_keypair_with_rng(network, &mut rng))
        .unzip();

    let mut global = GlobalConfig {
        network,
        public_keys,
        funding_transaction: None,
        anchoring_interval,
        ..Default::default()
    };

    let address = global.anchoring_address();

    let local_cfgs = private_keys
        .iter()
        .map(|sk| LocalConfig {
            rpc: Some(config.clone()),
            private_keys: hashmap!{ address.clone() => sk.clone() },
        })
        .collect();

    client.watch_address(&address, false).unwrap();
    let tx = client.send_to_address(&address, total_funds).unwrap();

    global.funding_transaction = Some(tx);
    (global, local_cfgs)
}

// Notorious test kit wrapper
#[derive(Debug)]
pub struct AnchoringTestKit {
    test_kit: TestKit,
    pub local_private_keys: Arc<RwLock<HashMap<btc::Address, btc::Privkey>>>,
    pub node_configs: Vec<LocalConfig>,
}

impl Deref for AnchoringTestKit {
    type Target = TestKit;

    fn deref(&self) -> &Self::Target {
        &self.test_kit
    }
}

impl DerefMut for AnchoringTestKit {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.test_kit
    }
}

impl AnchoringTestKit {
    pub fn new_with_testnet(
        validators_num: u16,
        total_funds: u64,
        anchoring_interval: u64,
    ) -> Self {
        let network = Network::Testnet;

        let rpc_config = BitcoinRpcConfig {
            host: env::var("ANCHORING_RELAY_HOST")
                .unwrap_or_else(|_| String::from("http://127.0.0.1:18332")),
            username: env::var("ANCHORING_USER")
                .ok()
                .or_else(|| Some(String::from("testnet"))),
            password: env::var("ANCHORING_PASSWORD")
                .ok()
                .or_else(|| Some(String::from("testnet"))),
        };

        let relay = Box::<BtcRelay>::from(BitcoinRpcClient::from(rpc_config.clone()));
        let (global, locals) = gen_anchoring_config(
            &rpc_config,
            network,
            validators_num,
            total_funds,
            anchoring_interval,
        );

        let local = locals[0].clone();

        let private_keys = Arc::new(RwLock::new(local.private_keys));

        let service =
            BtcAnchoringService::new(global.clone(), Arc::clone(&private_keys), Some(relay));

        let testkit = TestKitBuilder::validator()
            .with_service(service)
            .with_validators(validators_num)
            .with_logger()
            .create();

        Self {
            test_kit: testkit,
            local_private_keys: private_keys,
            node_configs: locals,
        }
    }

    pub fn renew_address(&mut self) {
        let schema = BtcAnchoringSchema::new(self.snapshot());

        if let BtcAnchoringState::Transition {
            actual_configuration,
            following_configuration,
        } = schema.actual_state()
        {
            let old_addr = actual_configuration.anchoring_address();
            let new_addr = following_configuration.anchoring_address();

            let pk = {
                let private_keys = self.local_private_keys.read().unwrap();
                private_keys.get(&old_addr).unwrap().clone()
            };

            if old_addr != new_addr {
                trace!("setting new pkey for addr {:?} ", new_addr);
                let mut private_keys = self.local_private_keys.write().unwrap();
                private_keys.insert(new_addr.clone(), pk.clone());

                for local_cfg in &mut self.node_configs.iter_mut() {
                    let pk = local_cfg.private_keys[&old_addr].clone();
                    local_cfg.private_keys.insert(new_addr.clone(), pk);
                }
            }
        }
    }

    fn get_local_cfg(&self, node: &TestNode) -> LocalConfig {
        self.node_configs[node.validator_id().unwrap().0 as usize].clone()
    }

    pub fn anchoring_us(&self) -> (TestNode, LocalConfig) {
        let node = self.test_kit.us();
        let cfg = self.get_local_cfg(node);
        (node.clone(), cfg)
    }

    pub fn anchoring_validators(&self) -> Vec<(TestNode, LocalConfig)> {
        let validators = self.test_kit.network().validators();
        validators
            .into_iter()
            .map(|validator| (validator.clone(), self.get_local_cfg(validator)))
            .collect::<Vec<(TestNode, LocalConfig)>>()
    }

    pub fn redeem_script(&self) -> RedeemScript {
        let fork = self.blockchain().fork();
        let schema = BtcAnchoringSchema::new(fork);

        schema.actual_state().actual_configuration().redeem_script()
    }

    pub fn anchoring_address(&self) -> btc::Address {
        let fork = self.blockchain().fork();
        let schema = BtcAnchoringSchema::new(fork);

        schema
            .actual_state()
            .actual_configuration()
            .anchoring_address()
    }

    pub fn rpc_client(&self) -> BitcoinRpcClient {
        let rpc_cfg = self.get_local_cfg(self.us()).rpc.unwrap();
        BitcoinRpcClient::from(rpc_cfg)
    }

    pub fn last_anchoring_tx(&self) -> Option<btc::Transaction> {
        let schema = BtcAnchoringSchema::new(self.snapshot());
        schema.anchoring_transactions_chain().last()
    }

    pub fn create_signature_tx_for_validators(
        &self,
        validators_num: u16,
    ) -> Result<Vec<Box<Transaction>>, btc::BuilderError> {
        let validators = self.network()
            .validators()
            .iter()
            .filter(|v| v != &self.us())
            .take(validators_num as usize);

        let mut signatures: Vec<Box<Transaction>> = vec![];

        let redeem_script = self.redeem_script();
        let mut signer = p2wsh::InputSigner::new(redeem_script.clone());

        for validator in validators {
            let validator_id = validator.validator_id().unwrap();
            let (public_key, private_key) = validator.service_keypair();

            let schema = BtcAnchoringSchema::new(self.snapshot());

            if let Some(p) = schema.actual_proposed_anchoring_transaction() {
                let (proposal, proposal_inputs) = p?;

                let address = anchoring_schema.actual_state().output_address();
                let privkey = &self.node_configs[validator_id.0 as usize].private_keys[&address];

                let pubkey = redeem_script.content().public_keys[validator_id.0 as usize];

                for (index, proposal_input) in proposal_inputs.iter().enumerate() {
                    let signature = signer
                        .sign_input(
                            TxInRef::new(proposal.as_ref(), index),
                            proposal_input.as_ref(),
                            privkey.0.secret_key(),
                        )
                        .unwrap();

                    let tx = Signature::new(
                        &public_key,
                        validator_id,
                        proposal.clone(),
                        index as u32,
                        signature.as_ref(),
                        &private_key,
                    );
                    signatures.push(tx.into());
                }
            }
        }
        Ok(signatures)
    }

    pub fn drop_validator_proposal(&mut self) -> TestNetworkConfiguration {
        let mut proposal = self.configuration_change_proposal();
        let mut validators = proposal.validators().to_vec();

        validators.pop();
        proposal.set_validators(validators);

        let config: GlobalConfig = proposal.service_config(BTC_ANCHORING_SERVICE_NAME);

        let mut keys = config.public_keys.clone();

        keys.pop();

        let service_configuration = GlobalConfig {
            public_keys: keys,
            ..config
        };
        proposal.set_service_config(BTC_ANCHORING_SERVICE_NAME, service_configuration);
        proposal
    }
}
