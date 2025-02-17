use std::net::IpAddr;
use std::ops::Not;
use std::pin::Pin;
use std::str::FromStr;

use futures::StreamExt;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tonic::metadata::MetadataMap;
use tonic_web::CorsGrpcWeb;
use tracing::{error, trace, warn};
use uuid::Uuid;

use opendut_carl_api::proto::services::peer_messaging_broker::{Downstream, Upstream};
use opendut_carl_api::proto::services::peer_messaging_broker::peer_messaging_broker_server::PeerMessagingBrokerServer;
use opendut_carl_api::proto::services::peer_messaging_broker::upstream;
use opendut_types::peer::PeerId;
use crate::peer::broker::{OpenError, PeerMessagingBrokerRef};

pub struct PeerMessagingBrokerFacade {
    peer_messaging_broker: PeerMessagingBrokerRef,
}

impl PeerMessagingBrokerFacade {
    pub fn new(peer_messaging_broker: PeerMessagingBrokerRef) -> Self {
        Self { peer_messaging_broker }
    }
    pub fn into_grpc_service(self) -> CorsGrpcWeb<PeerMessagingBrokerServer<Self>> {
        tonic_web::enable(PeerMessagingBrokerServer::new(self))
    }
}

#[tonic::async_trait]
impl opendut_carl_api::proto::services::peer_messaging_broker::peer_messaging_broker_server::PeerMessagingBroker for PeerMessagingBrokerFacade {

    type OpenStream = Pin<Box<dyn Stream<Item = Result<Downstream, Status>> + Send>>;

    #[tracing::instrument(skip(self, request), level="trace")]
    async fn open(&self, request: Request<Streaming<Upstream>>) -> Result<Response<Self::OpenStream>, Status> {

        let peer_id = extract_peer_id(request.metadata())
            .map_err(|message| {
                warn!("Error while parsing PeerId from client request: {message}");
                Status::invalid_argument(message)
            })?;

        let remote_host = extract_remote_host(request.metadata())
            .map_err(|message| {
                warn!("Error while parsing remote host address from client request: {message}");
                Status::invalid_argument(message)
            })?;


        let (tx_inbound, rx_outbound) = self.peer_messaging_broker.open(peer_id, remote_host).await
            .map_err(|cause| match cause {
                OpenError::PeerAlreadyConnected { .. } => Status::aborted(cause.to_string()),
                OpenError::SendApplyPeerConfiguration { .. } => Status::unavailable(cause.to_string()),
                OpenError::Persistence { .. } => Status::internal(cause.to_string()),
            })?;

        let mut inbound = request.into_inner();
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(upstream) => {
                        if let Some(message) = upstream.message {
                            if matches!(message, upstream::Message::Ping(_)).not() {
                                trace!("Received message from client <{}>: {:?}", peer_id, message);
                            }
                            tx_inbound.send(message).await.unwrap();
                        } else {
                            warn!("Ignoring empty message from client <{}>: {:?}", peer_id, upstream);
                        }
                    }
                    Err(status) => {
                        error!("Error: {:?}", status);
                    }
                }
            }
        });

        let outbound = ReceiverStream::new(rx_outbound)
            .map(|downstream| {
                Ok(downstream)
            });

        Ok(Response::new(Box::pin(outbound)))
    }
}


fn extract_peer_id(metadata: &MetadataMap) -> Result<PeerId, UserError> {
    let peer_id = PeerId::from(
        Uuid::parse_str(
            metadata
                .get("id")
                .ok_or("Client should have sent an ID")?
                .to_str()
                .map_err(|_| "Client ID should be a valid string")?
        ).map_err(|_| "Client ID should be a valid UUID")?
    );
    Ok(peer_id)
}

fn extract_remote_host(metadata: &MetadataMap) -> Result<IpAddr, UserError> {
    let remote_host = IpAddr::from_str(
        metadata
            .get("remote-host")
            .ok_or("Client should have sent a remote host address")?
            .to_str()
            .map_err(|_| "Remote host address should be a valid string")?
    ).map_err(|_| "Remote host address should be a valid IP address")?;

    Ok(remote_host)
}


type UserError = String;
