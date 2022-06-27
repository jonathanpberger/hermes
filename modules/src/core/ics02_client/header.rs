use core::ops::Deref;

use ibc_proto::google::protobuf::Any as ProtoAny;
use serde_derive::{Deserialize, Serialize};
use subtle_encoding::hex;
use tendermint_proto::Protobuf;

use crate::clients::ics07_tendermint::header::{decode_header, Header as TendermintHeader};
use crate::core::ics02_client::client_type::ClientType;
use crate::core::ics02_client::error::Error;
use crate::dynamic_typing::AsAny;
#[cfg(any(test, feature = "mocks"))]
use crate::mock::header::MockHeader;
use crate::prelude::*;
use crate::timestamp::Timestamp;
use crate::Height;

pub const TENDERMINT_HEADER_TYPE_URL: &str = "/ibc.lightclients.tendermint.v1.Header";
pub const MOCK_HEADER_TYPE_URL: &str = "/ibc.mock.Header";

/// Abstract of consensus state update information
pub trait Header: core::fmt::Debug + Send + Sync + AsAny {
    /// The type of client (eg. Tendermint)
    fn client_type(&self) -> ClientType;

    /// The height of the consensus state
    fn height(&self) -> Height;

    /// The timestamp of the consensus state
    fn timestamp(&self) -> Timestamp;

    /// Consumes the given instance and returns a heap allocated instance
    fn boxed(self) -> Box<dyn Header>
    where
        Self: Sized,
    {
        Box::new(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum AnyHeader {
    Tendermint(TendermintHeader),

    #[cfg(any(test, feature = "mocks"))]
    Mock(MockHeader),
}

impl Header for AnyHeader {
    fn client_type(&self) -> ClientType {
        match self {
            Self::Tendermint(header) => header.client_type(),

            #[cfg(any(test, feature = "mocks"))]
            Self::Mock(header) => header.client_type(),
        }
    }

    fn height(&self) -> Height {
        match self {
            Self::Tendermint(header) => header.height(),

            #[cfg(any(test, feature = "mocks"))]
            Self::Mock(header) => header.height(),
        }
    }

    fn timestamp(&self) -> Timestamp {
        match self {
            Self::Tendermint(header) => header.timestamp(),
            #[cfg(any(test, feature = "mocks"))]
            Self::Mock(header) => header.timestamp(),
        }
    }
}

impl AnyHeader {
    pub fn encode_to_string(&self) -> String {
        let buf = Protobuf::encode_vec(self).expect("encoding shouldn't fail");
        let encoded = hex::encode(buf);
        String::from_utf8(encoded).expect("hex-encoded string should always be valid UTF-8")
    }
}

impl Protobuf<ProtoAny> for AnyHeader {}

impl TryFrom<ProtoAny> for AnyHeader {
    type Error = Error;

    fn try_from(raw: ProtoAny) -> Result<Self, Error> {
        match raw.type_url.as_str() {
            TENDERMINT_HEADER_TYPE_URL => {
                let val = decode_header(raw.value.deref())?;

                Ok(AnyHeader::Tendermint(val))
            }

            #[cfg(any(test, feature = "mocks"))]
            MOCK_HEADER_TYPE_URL => Ok(AnyHeader::Mock(
                MockHeader::decode_vec(&raw.value).map_err(Error::invalid_raw_header)?,
            )),

            _ => Err(Error::unknown_header_type(raw.type_url)),
        }
    }
}

impl From<AnyHeader> for ProtoAny {
    fn from(value: AnyHeader) -> Self {
        match value {
            AnyHeader::Tendermint(header) => ProtoAny {
                type_url: TENDERMINT_HEADER_TYPE_URL.to_string(),
                value: header
                    .encode_vec()
                    .expect("encoding to `Any` from `AnyHeader::Tendermint`"),
            },
            #[cfg(any(test, feature = "mocks"))]
            AnyHeader::Mock(header) => ProtoAny {
                type_url: MOCK_HEADER_TYPE_URL.to_string(),
                value: header
                    .encode_vec()
                    .expect("encoding to `Any` from `AnyHeader::Mock`"),
            },
        }
    }
}

#[cfg(any(test, feature = "mocks"))]
impl From<MockHeader> for AnyHeader {
    fn from(header: MockHeader) -> Self {
        Self::Mock(header)
    }
}

impl From<TendermintHeader> for AnyHeader {
    fn from(header: TendermintHeader) -> Self {
        Self::Tendermint(header)
    }
}
