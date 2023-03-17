use crate::{
    graphql_api::service::Database,
    query::{
        CoinQueryData,
        MessageQueryData,
    },
};
use fuel_core_storage::{
    iter::IterDirection,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_types::{
    entities::resource::{
        Resource,
        ResourceId,
    },
    fuel_tx::UtxoId,
    fuel_types::{
        Address,
        AssetId,
        MessageId,
    },
};
use itertools::Itertools;
use std::collections::HashSet;

/// At least required `target` of the query per asset's `id` with `max` resources.
#[derive(Clone)]
pub struct AssetSpendTarget {
    pub id: AssetId,
    pub target: u64,
    pub max: usize,
}

impl AssetSpendTarget {
    pub fn new(id: AssetId, target: u64, max: u64) -> Self {
        Self {
            id,
            target,
            max: max as usize,
        }
    }
}

#[derive(Default)]
pub struct Exclude {
    pub utxos: HashSet<UtxoId>,
    pub messages: HashSet<MessageId>,
}

impl Exclude {
    pub fn new(ids: Vec<ResourceId>) -> Self {
        let mut instance = Self::default();

        for id in ids.into_iter() {
            match id {
                ResourceId::Utxo(utxo) => instance.utxos.insert(utxo),
                ResourceId::Message(message) => instance.messages.insert(message),
            };
        }

        instance
    }
}

pub struct AssetsQuery<'a> {
    pub owner: &'a Address,
    pub assets: Option<HashSet<&'a AssetId>>,
    pub exclude: Option<&'a Exclude>,
    pub database: &'a Database,
}

impl<'a> AssetsQuery<'a> {
    pub fn new(
        owner: &'a Address,
        assets: Option<HashSet<&'a AssetId>>,
        exclude: Option<&'a Exclude>,
        database: &'a Database,
    ) -> Self {
        Self {
            owner,
            assets,
            exclude,
            database,
        }
    }

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) resources of the `owner`.
    ///
    /// # Note: The resources of different type are not grouped by the `asset_id`.
    // TODO: Optimize this by creating an index
    //  https://github.com/FuelLabs/fuel-core/issues/588
    pub fn resources(&self) -> impl Iterator<Item = StorageResult<Resource>> + '_ {
        let coins_iter = self
            .database
            .owned_coins_ids(self.owner, None, IterDirection::Forward)
            .filter_ok(|id| {
                if let Some(exclude) = self.exclude {
                    !exclude.utxos.contains(id)
                } else {
                    true
                }
            })
            .map(move |res| {
                res.map_err(StorageError::from).and_then(|id| {
                    let coin = self.database.coin(id)?;

                    Ok(Resource::Coin(coin))
                })
            })
            .filter_ok(|coin| {
                if let Resource::Coin(coin) = coin {
                    self.assets
                        .as_ref()
                        .map(|assets| assets.contains(&coin.asset_id))
                        .unwrap_or(true)
                } else {
                    true
                }
            });

        let messages_iter = self
            .database
            .owned_message_ids(self.owner, None, IterDirection::Forward)
            .filter_ok(|id| {
                if let Some(exclude) = self.exclude {
                    !exclude.messages.contains(id)
                } else {
                    true
                }
            })
            .map(move |res| {
                res.and_then(|id| {
                    let message = self.database.message(&id)?;
                    Ok(Resource::Message(message))
                })
            });

        coins_iter.chain(messages_iter.take_while(|_| {
            self.assets
                .as_ref()
                .map(|assets| assets.contains(&AssetId::BASE))
                .unwrap_or(true)
        }))
    }
}

pub struct AssetQuery<'a> {
    pub owner: &'a Address,
    pub asset: &'a AssetSpendTarget,
    pub exclude: Option<&'a Exclude>,
    pub database: &'a Database,
    query: AssetsQuery<'a>,
}

impl<'a> AssetQuery<'a> {
    pub fn new(
        owner: &'a Address,
        asset: &'a AssetSpendTarget,
        exclude: Option<&'a Exclude>,
        database: &'a Database,
    ) -> Self {
        let mut allowed = HashSet::new();
        allowed.insert(&asset.id);
        Self {
            owner,
            asset,
            exclude,
            database,
            query: AssetsQuery::new(owner, Some(allowed), exclude, database),
        }
    }

    /// Returns the iterator over all valid(spendable, allowed by `exclude`) resources of the `owner`
    /// for the `asset_id`.
    pub fn resources(&self) -> impl Iterator<Item = StorageResult<Resource>> + '_ {
        self.query.resources()
    }
}