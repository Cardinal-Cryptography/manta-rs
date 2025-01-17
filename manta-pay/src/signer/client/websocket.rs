// Copyright 2019-2022 Manta Network.
// This file is part of manta-rs.
//
// manta-rs is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// manta-rs is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with manta-rs.  If not, see <http://www.gnu.org/licenses/>.

//! Signer WebSocket Client Implementation

// TODO: Make this code work on WASM and non-WASM by choosing the correct dependency library.

use crate::{
    config::{utxo::Address, Config},
    signer::{
        AssetMetadata, Checkpoint, GetRequest, IdentityRequest, IdentityResponse, SignError,
        SignRequest, SignResponse, SignWithTransactionDataResult, SyncError, SyncRequest,
        SyncResponse, TransactionDataRequest, TransactionDataResponse,
    },
};
use alloc::boxed::Box;
use core::marker::Unpin;
use futures::{SinkExt, StreamExt};
use manta_accounting::wallet::{self, signer};
use manta_util::{
    from_variant,
    future::LocalBoxFutureResult,
    serde::{de::DeserializeOwned, Deserialize, Serialize},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

/// Web Socket Error
pub type WebSocketError = tungstenite::error::Error;

/// Client Error
#[derive(Debug)]
pub enum Error {
    /// Invalid Message Format
    ///
    /// The message received from the WebSocket connection was not a [`Message::Text`].
    InvalidMessageFormat,

    /// End of Stream Error
    ///
    /// The WebSocket stream was closed while waiting for the next message.
    EndOfStream,

    /// Serialization Error
    SerializationError(serde_json::Error),

    /// WebSocket Error
    WebSocket(WebSocketError),
}

from_variant!(Error, SerializationError, serde_json::Error);
from_variant!(Error, WebSocket, WebSocketError);

/// Request
#[cfg_attr(
    feature = "serde",
    derive(Deserialize, Serialize),
    serde(crate = "manta_util::serde", deny_unknown_fields)
)]
#[derive(derivative::Derivative)]
#[derivative(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Request<R> {
    /// Request Command
    ///
    /// This command is used by the server to decide which command to execute the request on, and to
    /// parse the request correctly from the serialized data.
    pub command: &'static str,

    /// Request Body
    pub request: R,
}

/// Wallet Associated to [`Client`]
pub type Wallet<L> = wallet::Wallet<Config, L, Client>;

/// WebSocket Client
#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct Client(WebSocketStream<MaybeTlsStream<TcpStream>>);

impl Client {
    /// Builds a new [`Client`] from `url`.
    #[inline]
    pub async fn new<U>(url: U) -> Result<Self, WebSocketError>
    where
        U: IntoClientRequest + Unpin,
    {
        Ok(Self(connect_async(url).await?.0))
    }

    /// Sends a `request` for the given `command` along the channel and waits for the response.
    #[inline]
    async fn send<S, D>(&mut self, command: &'static str, request: S) -> Result<D, Error>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.0
            .send(Message::Text(serde_json::to_string(&Request {
                command,
                request,
            })?))
            .await?;
        match self.0.next().await {
            Some(Ok(Message::Text(message))) => Ok(serde_json::from_str(&message)?),
            Some(Ok(_)) => Err(Error::InvalidMessageFormat),
            Some(Err(err)) => Err(Error::WebSocket(err)),
            _ => Err(Error::EndOfStream),
        }
    }
}

impl signer::Connection<Config> for Client {
    type AssetMetadata = AssetMetadata;
    type Checkpoint = Checkpoint;
    type Error = Error;

    #[inline]
    fn sync(
        &mut self,
        request: SyncRequest,
    ) -> LocalBoxFutureResult<Result<SyncResponse, SyncError>, Self::Error> {
        Box::pin(async move { self.send("sync", request).await })
    }

    #[inline]
    fn sign(
        &mut self,
        request: SignRequest,
    ) -> LocalBoxFutureResult<Result<SignResponse, SignError>, Self::Error> {
        Box::pin(async move { self.send("sign", request).await })
    }

    #[inline]
    fn address(&mut self) -> LocalBoxFutureResult<Option<Address>, Self::Error> {
        Box::pin(async move { self.send("address", GetRequest::Get).await })
    }

    #[inline]
    fn transaction_data(
        &mut self,
        request: TransactionDataRequest,
    ) -> LocalBoxFutureResult<TransactionDataResponse, Self::Error> {
        Box::pin(async move { self.send("transaction_data", request).await })
    }

    #[inline]
    fn identity_proof(
        &mut self,
        request: IdentityRequest,
    ) -> LocalBoxFutureResult<IdentityResponse, Self::Error> {
        Box::pin(async move { self.send("identity", request).await })
    }

    #[inline]
    fn sign_with_transaction_data(
        &mut self,
        request: SignRequest,
    ) -> LocalBoxFutureResult<SignWithTransactionDataResult, Self::Error> {
        Box::pin(async move { self.send("sign_with_transaction_data", request).await })
    }
}
