use serde_json;

use exonum::storage::StorageValue;
use exonum::crypto::{Hash, hash};

use details::btc;
use details::btc::transactions::FundingTx;

/// Public part of anchoring service configuration stored in blockchain.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct AnchoringConfig {
    /// Public keys validators of which the current `anchoring` address can be obtained.
    pub validators: Vec<btc::PublicKey>,
    /// The transaction that funds `anchoring` address.
    /// If the chain of transaction is empty it will be a first transaction in the chain.
    /// Note: you must specify a suitable transaction before the network launching.
    pub funding_tx: Option<FundingTx>,
    /// A fee for each transaction in chain
    pub fee: u64,
    /// The frequency in blocks with which occurs the generation of a new `anchoring`
    /// transactions in chain.
    pub frequency: u64,
    /// The minimum number of confirmations in bitcoin network for the transition to a
    /// new `anchoring` address.
    pub utxo_confirmations: u64,
    /// The current bitcoin network type
    #[serde(serialize_with = "btc_network_to_str", deserialize_with = "btc_network_from_str")]
    pub network: btc::Network,
}

impl Default for AnchoringConfig {
    fn default() -> AnchoringConfig {
        AnchoringConfig {
            validators: vec![],
            funding_tx: None,
            fee: 1000,
            frequency: 500,
            utxo_confirmations: 5,
            network: btc::Network::Testnet,
        }
    }
}

impl AnchoringConfig {
    /// Creates anchoring configuration for the given keypair without funding transaction.
    /// This is usable for deploying procedure when the network participants exchanges
    /// the public configuration before launching.
    /// Do not forget to send funding transaction to final multisig address
    /// add it to final configuration.
    pub fn new(network: btc::Network, public_key: btc::PublicKey) -> AnchoringConfig {
        AnchoringConfig {
            validators: vec![public_key],
            network: network,
            ..Default::default()
        }
    }

    /// Creates default anchoring configuration from given public keys and funding transaction
    /// which were created earlier by other way.
    pub fn new_with_funding_tx(network: btc::Network,
                               validators: Vec<btc::PublicKey>,
                               tx: FundingTx)
                               -> AnchoringConfig {
        AnchoringConfig {
            validators: validators,
            funding_tx: Some(tx),
            network: network,
            ..Default::default()
        }
    }

    #[doc(hidden)]
    /// Creates compressed `redeem_script` from public keys in config.
    pub fn redeem_script(&self) -> (btc::RedeemScript, btc::Address) {
        let majority_count = self.majority_count();
        let redeem_script = btc::RedeemScript::from_pubkeys(self.validators.iter(), majority_count)
            .compressed(self.network);
        let addr = btc::Address::from_script(&redeem_script, self.network);
        (redeem_script, addr)
    }

    #[doc(hidden)]
    /// Returns the latest height below the given `height` which needs to be anchored
    pub fn latest_anchoring_height(&self, height: u64) -> u64 {
        height - height % self.frequency as u64
    }

    #[doc(hidden)]
    /// For test purpose only
    pub fn majority_count(&self) -> u8 {
        ::majority_count(self.validators.len() as u8)
    }

    pub fn funding_tx(&self) -> &FundingTx {
        self.funding_tx
            .as_ref()
            .expect("You need to specify suitable funding_tx")
    }
}

fn btc_network_to_str<S>(network: &btc::Network, ser: &mut S) -> Result<(), S::Error>
    where S: ::serde::Serializer
{
    match *network {
        btc::Network::Bitcoin => ser.serialize_str("bitcoin"),
        btc::Network::Testnet => ser.serialize_str("testnet"),
    }
}

fn btc_network_from_str<D>(deserializer: &mut D) -> Result<btc::Network, D::Error>
    where D: ::serde::Deserializer
{
    let s: String = ::serde::Deserialize::deserialize(deserializer)?;
    match s.as_str() {
        "bitcoin" => Ok(btc::Network::Bitcoin),
        "testnet" => Ok(btc::Network::Testnet),
        _ => Err(::serde::de::Error::invalid_value("Wrong network")),
    }
}

impl StorageValue for AnchoringConfig {
    fn serialize(self) -> Vec<u8> {
        serde_json::to_vec(&self).unwrap()
    }

    fn deserialize(v: Vec<u8>) -> Self {
        serde_json::from_slice(v.as_slice()).unwrap()
    }

    fn hash(&self) -> Hash {
        hash(serde_json::to_vec(&self).unwrap().as_slice())
    }
}
