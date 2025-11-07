// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::{
    connection_package::ConnectionPackage, connection_package::VersionedConnectionPackage,
    connection_package_v1::ConnectionPackageV1,
};
use serde::{Deserialize, Serialize};

pub(crate) mod persistence;

#[derive(Deserialize)]
pub(in crate::auth_service) enum StorableConnectionPackage {
    V1(ConnectionPackageV1),
    V2(ConnectionPackage),
}

impl From<StorableConnectionPackage> for VersionedConnectionPackage {
    fn from(connection_package: StorableConnectionPackage) -> Self {
        match connection_package {
            StorableConnectionPackage::V1(cp_v1) => VersionedConnectionPackage::V1(cp_v1),
            StorableConnectionPackage::V2(connection_package) => {
                VersionedConnectionPackage::V2(connection_package)
            }
        }
    }
}

impl From<VersionedConnectionPackage> for StorableConnectionPackage {
    fn from(connection_package: VersionedConnectionPackage) -> Self {
        match connection_package {
            VersionedConnectionPackage::V1(cp_v1) => StorableConnectionPackage::V1(cp_v1),
            VersionedConnectionPackage::V2(connection_package) => {
                StorableConnectionPackage::V2(connection_package)
            }
        }
    }
}

#[derive(Serialize)]
pub(in crate::auth_service) enum StorableConnectionPackageRef<'a> {
    V1(&'a ConnectionPackageV1),
    V2(&'a ConnectionPackage),
}

impl<'a> From<&'a VersionedConnectionPackage> for StorableConnectionPackageRef<'a> {
    fn from(connection_package: &'a VersionedConnectionPackage) -> Self {
        match connection_package {
            VersionedConnectionPackage::V1(cp_v1) => StorableConnectionPackageRef::V1(cp_v1),
            VersionedConnectionPackage::V2(cp_v2) => StorableConnectionPackageRef::V2(cp_v2),
        }
    }
}
