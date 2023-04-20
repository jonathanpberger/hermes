use core::str::FromStr;

use alloc::string::ToString;
use alloc::vec::Vec;
use ibc_proto::ibc::core::channel::v1::UpgradeFields as RawUpgradeFields;
use ibc_proto::protobuf::Protobuf;
use itertools::Itertools;

use crate::core::ics04_channel::channel::Ordering;
use crate::core::ics04_channel::error::Error as ChannelError;
use crate::core::ics04_channel::version::Version;
use crate::core::ics24_host::identifier::ConnectionId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeFields {
    ordering: Ordering,
    connection_hops: Vec<ConnectionId>,
    version: Version,
}

impl UpgradeFields {
    pub fn new(ordering: Ordering, connection_hops: Vec<ConnectionId>, version: Version) -> Self {
        Self {
            ordering,
            connection_hops,
            version,
        }
    }
}

impl Protobuf<RawUpgradeFields> for UpgradeFields {}

impl TryFrom<RawUpgradeFields> for UpgradeFields {
    type Error = ChannelError;

    fn try_from(value: RawUpgradeFields) -> Result<Self, Self::Error> {
        let ordering = Ordering::from_i32(value.ordering)?;

        let (connection_hops, failures): (Vec<_>, Vec<_>) = value
            .connection_hops
            .iter()
            .partition_map(|id| match ConnectionId::from_str(id) {
                Ok(connection_id) => itertools::Either::Left(connection_id),
                Err(e) => itertools::Either::Right((id.clone(), e)),
            });

        if !failures.is_empty() {
            return Err(Self::Error::parse_connection_hops_vector(failures));
        }

        let version = Version::from(value.version);

        Ok(Self::new(ordering, connection_hops, version))
    }
}

impl From<UpgradeFields> for RawUpgradeFields {
    fn from(value: UpgradeFields) -> Self {
        let raw_connection_hops = value
            .connection_hops
            .iter()
            .map(|id| id.to_string())
            .collect();
        Self {
            ordering: value.ordering as i32,
            connection_hops: raw_connection_hops,
            version: value.version.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use alloc::{string::ToString, vec};
    use ibc_proto::ibc::core::channel::v1::UpgradeFields as RawUpgradeFields;

    pub fn get_dummy_upgrade_fields() -> RawUpgradeFields {
        RawUpgradeFields {
            ordering: 1,
            connection_hops: vec![],
            version: "ics20".to_string(),
        }
    }
}