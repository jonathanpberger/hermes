use flex_error::{define_error, TraceError};

use crate::core::ics02_client::error::Error as Ics02Error;
use crate::core::ics24_host::error::ValidationError;

use crate::Height;

define_error! {
    #[derive(Debug, PartialEq, Eq)]
    Error {
        InvalidRawClientState
            { reason: String }
            |e| { format_args!("invalid raw client state: {}", e.reason) },

        InvalidChainIdentifier
            [ ValidationError ]
            |_| { "invalid chain identifier" },

        MissingHeight
            |_| { "missing height" },

        InvalidChainId
            { raw_value: String }
            [ ValidationError ]
            |e| { format_args!("invalid chain identifier: {}", e.raw_value) },

        InvalidRawHeight
            { raw_height: u64 }
            |e| { format_args!("invalid raw height: {}", e.raw_height) },

        Decode
            [ TraceError<prost::DecodeError> ]
            |_| { "decode error" },

        InsufficientHeight
            {
                latest_height: Height,
                target_height: Height,
            }
            |e| {
                format_args!("the height is insufficient: latest_height={0} target_height={1}",
                    e.latest_height, e.target_height)
            },
    }
}

impl From<Error> for Ics02Error {
    fn from(e: Error) -> Self {
        Self::client_specific(e.to_string())
    }
}